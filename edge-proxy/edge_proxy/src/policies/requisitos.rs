use std::time::Duration;

use itertools::Itertools;
use ringbuffer::RingBuffer;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use edge_proxy_lib::{
    policy::{Policy, Receiver, Request, RequestContext, Sender},
    server::{Endpoint, Endpoints}
};
use uuid::Uuid;
use anyhow::Result;

use crate::utils::{calcular_hw, cola_estimada_ms, escoger_n, promedio_latencia, Order};

use super::{process_locally, read_models, Model, SimpleContext, TritonEndpoint, TritonEndpoints, TritonRequest};



#[derive(Debug, Clone, Default)]
pub struct Requisitos {
    models: Vec<Model>
}

impl Requisitos {
    pub fn new(path: &str) -> Result<Self> {
        Ok(Self {
            models: read_models(path)?
        })
    }
}

pub fn est_tiempo_para_acc(ep: &TritonEndpoint, acc: u32) -> Option<u32> {
    
    ep.last_results
        .iter()
        .filter(|r| r.duration.is_some())
        .rev()
        .min_by_key(|res| res.context.accuracy.abs_diff(acc))
        .map(|r| r.duration.unwrap().as_millis() as u32)
    
}

impl Policy<SimpleContext> for Requisitos {

    async fn choose_target(&self, request: &TritonRequest, endps: &TritonEndpoints) -> Uuid {

        // Quitar nodos anteriores para que no haya ciclos.
        let nodes  = endps.iter()
            .filter(|(uuid, _)| !request.previous_nodes.contains(uuid))
            .collect_vec();
        
        // Sacar nodos que han cumplido los requisitos anteriormente.
        let cumplen = nodes.iter()
            .filter(|(_, ep)| {
                let est_tiempo = est_tiempo_para_acc(ep, request.context.accuracy);
                est_tiempo.is_some_and(|t| t < request.context.priority)
            });
        
        // Si alguno ha cumplido, enviar al que tenga menos trabajo.
        if let Some(ep) = cumplen.min_by_key(|(_, ep)| cola_estimada_ms(ep)) {
            return *ep.0;
        }
        
        // Si hay nodos no usados, probar con el más potente.
        let no_probados = nodes.iter().filter(|(_, ep)| ep.last_results.len() == 0);
        if let Some(ep) = no_probados.min_by_key(|(_, ep)| calcular_hw(ep)) {
            return *ep.0;
        }

        let no_completo = nodes.iter()
            .filter(|(_, ep)| ep.last_results.len() < 3)
            .max_by_key(|(_, ep)| calcular_hw(ep));

        if let Some(ep) = no_completo {
            return *ep.0;
        }


        //let antiguo = nodes.iter()
        //    .filter(|(_, ep)| {
        //        let last_ind = ep.last_results.len() - 1;
        //        let ultimo_envio = ep.last_results[last_ind].instant;
        //        // Miramos que no se haya enviado desde hace 30? segundos.
        //        ultimo_envio.elapsed() >= Duration::from_secs(30)
        //    })
        //    .min_by_key(|(_, ep)| cola_estimada_ms(ep));

        // Como última opción, se envia al que menos tiempo de respuesta ha dado anteriormente.
        *nodes.iter().min_by_key(|(_, ep)| promedio_latencia(ep)).unwrap().0
    }
    
    async fn process_locally(&self, request: &TritonRequest) -> Result<Vec<u8>> {
        
        // Coger el modelo más rapido con accuracy >= a la pedida.
        // En caso de que no haya ningun modelo con accuracy suficiente, usar el
        // que más se acerque.
        let model = self.models.iter()
            .filter(|m| m.accuracy >= request.context.accuracy)
            .min_by_key(|m| m.perf)
            .unwrap_or(self.models.iter().max_by_key(|m| m.accuracy).unwrap());

        process_locally(&request, model).await
    }
}
