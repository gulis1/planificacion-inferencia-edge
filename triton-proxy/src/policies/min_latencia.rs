use ringbuffer::RingBuffer;
use uuid::Uuid;
use crate::{model::Model, server::{Endpoint, Endpoints}};
use super::{Policy, Request};

#[derive(Default)]
/// Reglas:
/// - Si hay algun endpoint que no se ha usado nunca, escoge ese.
/// - Si no, escoge que tenga una latencia media (5 ultimos intentos) más baja.
///
/// **Problema**: si localhost se considera óptim y le spameo peticiones, 
/// localhost se va a sobrecargar, y se empezará a enviar a otro vecino.
/// Pero cuando localhost vuelve a la normalidad, ya no se le vuelven a enviar
/// peticiones.
pub struct MinLatencia;

impl Policy for MinLatencia {

    async fn choose_target(&self, request: &Request, endpts: &Endpoints) -> Uuid {

        log::info!("MIERDA: {:?}", request.previous_nodes);
        let node_uuid = endpts.iter()
            // Para no hacer un ciclo.
            .filter(|ep| !request.previous_nodes.contains(ep.0))
            .min_by_key(|(_, ep)| calcular_peso(ep))
            .map(|(uuid, _)| *uuid)
            .unwrap();

       
       node_uuid
    }

    fn choose_model<'a>(&self, _request: &Request, models: &'a [Model]) -> &'a Model {
        models.iter()
            .max_by_key(|model| model.perf)
            .unwrap()
    }
}

/// Devuelve 0 si no se ha usado nunca.
fn calcular_peso(endp: &Endpoint) -> u32 {
    
    let numero_endps = endp.last_results.len() as u32;
    let sum_latencia: u32 = endp.last_results.iter()
        .map(|res| {
            match res {
                Ok(dur) => dur.as_millis() as u32,
                Err(_) => 10 * 1000 // 10 segundos si hubo fallo.
            }
        })
        .sum();
     
    let promedio = sum_latencia.checked_div(numero_endps).unwrap_or(0);
    log::info!("Promedio latencias pod {}: {}", endp.name, promedio);
    promedio
}
