use itertools::Itertools;
use ringbuffer::RingBuffer;
use triton_proxy_lib::{
    policy::{Policy, Request}, 
    server::{Endpoint, Endpoints}
};
use uuid::Uuid;
use crate::policies::process_locally;
use super::{read_models, Model, SimpleContext};
use anyhow::Result;

enum Order {
    Ascending(usize),
    Descending(usize)
}


#[derive(Debug, Clone, Default)]
pub struct MinQueue {
    models: Vec<Model> 
}

impl MinQueue {
    pub fn new(path: &str) -> Result<Self> {
        Ok(Self {
            models: read_models(path)?
        })
    }
}

impl Policy<SimpleContext> for MinQueue {
    
    async fn choose_target(&self, request: &Request<SimpleContext>, endps: &Endpoints) -> Uuid {
        
        let nodes = endps.iter()
            .filter(|(uuid, _)| !request.previous_nodes.contains(uuid));
        
        let target = match (request.context.priority, request.context.accuracy) {
            
            // Prioridad alta
            (1.., _) => {
                //log::info!("Política: petición prioridad alta.");
                //// Escoger (3) con menores tiempos de respuesta anteriores.
                //let mas_rapidos = escoger_n(Order::Ascending(3), nodes, promedio_latencia);
                //log::info!("Nodos más rápidos escogidos: {:#?}", mas_rapidos);
                //// De los (3), escoger el que tenga menor carga de trabajo.
                //mas_rapidos.iter()
                //    .min_by_key(|(_uuid, ep)| carga_trabajo(ep))
                //    .unwrap_or(&mas_rapidos[0])
                //    .0
                let mas_potentes = escoger_n(Order::Descending(2), nodes, calcular_hw);
                log::info!("Nodos más potentes escogidos: {:#?}", mas_potentes);
                mas_potentes.iter()
                    .min_by_key(|(_uuid, ep)| promedio_latencia(ep))
                    .unwrap_or(&mas_potentes[0])
                    .0

            }

            // Prioridad baja, accuracy baja
            (0, 0) => {
                log::info!("Política: petición prioridad baja accuracy baja.");
                // Enviar al que menos carga de trabajo tenga.
                *nodes
                    .min_by_key(|(_uuid, ep)| carga_trabajo(ep))
                    .unwrap()
                    .0
            },

            // Prioridad baja, accuracy alta
            (0, 1..) => {
                log::info!("Política: petición prioridad baja accuracy alta.");
                // 1. Escoger (3) nodos más potentes
                let mas_potentes = escoger_n(Order::Descending(3), nodes, calcular_hw);
                log::info!("Nodos más potentes escogidos: {:#?}", mas_potentes);
                // 2. De los (3), escoger el que tenga menor carga de trabajo.
                mas_potentes.iter()
                    .min_by_key(|(_uuid, ep)| carga_trabajo(ep))
                    .unwrap_or(&mas_potentes[0])
                    .0
            },
        };
        
        target
    }

    async fn process_locally(&self, request: &Request<SimpleContext>) -> Result<Vec<u8>> {
        
        let model = self.choose_model(request);
        process_locally(request, model).await
    }

}

impl<'a> MinQueue {

    fn choose_model(&'a self, request: &Request<SimpleContext>) -> &'a Model {
        
        let model = match (request.context.priority, request.context.accuracy) {

            // Accuracy baja
            (_, 0) => {
                // Modelo más rápido.
                self.models.iter()
                    .max_by_key(|m| m.perf)
                    .unwrap()
            },

            // Prioridad baja, accuracy alta.
            (0, _) => {
                // Modelo con más accuracy
                self.models.iter()
                    .max_by_key(|m| m.accuracy)
                    .unwrap()
            },

            (1.., 1..) => {
                // De los (3) modelos con más accuracy, se escoge el más rápido.
                self.models.iter()
                    .sorted_by_key(|m| m.accuracy)
                    .rev()
                    .take(3)
                    .max_by_key(|m| m.perf)
                    .unwrap()
            }
        };

        log::info!("Chosen model {} for request {}", model.name, request.id);
        model
    }
}


fn escoger_n<'a, I, F>(order: Order, endps: I, mut key: F) -> Vec<(Uuid, &'a Endpoint)>
    where I: Iterator<Item = (&'a Uuid, &'a Endpoint)>,
          F: FnMut(&Endpoint) -> u64
{
    let iter = endps
        .sorted_by_key(|(_uuid, ep)| key(ep))
        .map(|(uuid, ep)| (*uuid, ep));

    let res = match order {
        Order::Ascending(n) => iter.take(n).collect_vec(),
        Order::Descending(n) => iter.rev().take(n).collect_vec()
    };
    res
}

/// Devuelve 0 si no se ha usado nunca.
fn promedio_latencia(endp: &Endpoint) -> u64 {
    
    let numero_endps = endp.last_results.len() as u64;
    let sum_latencia: u64 = endp.last_results.iter()
        .map(|res| {
            match res {
                Ok(dur) => dur.as_millis() as u64,
                Err(_) => 10 * 1000 // 10 segundos si hubo fallo.
            }
        })
        .sum();

    // Default 0, porque si no nunca se probaría el endpoint. 
    let promedio = sum_latencia.checked_div(numero_endps).unwrap_or(0);
    log::info!("Promedio latencias pod {}: {}", endp.name, promedio);
    promedio
}


fn calcular_hw(ep: &Endpoint) -> u64 {
    let hw_info = &ep.hw_info.as_ref();

    let cpu_cores = hw_info
        .and_then(|info| info.get("physical_cores"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let gpu_cores = hw_info
        .and_then(|info| info.get("gpus"))
        .and_then(|v| v.as_array())
        .map(|gpus| {
            gpus.iter()
            .map(|gpu| {
                gpu.get("core_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
            })
            .sum()
        })
        .unwrap_or(0);
        
    
    let hw_score = cpu_cores + (gpu_cores / 10);
    log::info!("score hw de {}: {}", ep.name, hw_score);
    hw_score
}

#[inline]
fn carga_trabajo(ep: &Endpoint) -> u64 {
    let carga = ep.metrics
        .as_ref()
        .and_then(|metr| metr.get("queue_avg_5m"))
        .and_then(|v| v.as_f64())
        .map(|v| v as u64)
        .unwrap_or(u64::MAX);

    log::info!("Carga trabajo de {}: {}", ep.name, carga);
    carga
}
