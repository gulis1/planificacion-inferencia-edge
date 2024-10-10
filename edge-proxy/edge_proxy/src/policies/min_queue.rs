use itertools::Itertools;
use ringbuffer::RingBuffer;
use edge_proxy_lib::{
    policy::{Policy, Request}, 
    server::{Endpoint, Endpoints}
};
use uuid::Uuid;
use crate::{policies::process_locally, utils::{calcular_hw, cola_estimada_ms, escoger_n, promedio_latencia, Order}};
use super::{read_models, Model, SimpleContext};
use anyhow::Result;

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
    
    async fn choose_target(&self, request: &Request<SimpleContext>, endps: &Endpoints<SimpleContext>) -> Uuid {
        
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
                    .min_by_key(|(_uuid, ep)| cola_estimada_ms(ep))
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
                    .min_by_key(|(_uuid, ep)| cola_estimada_ms(ep))
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



