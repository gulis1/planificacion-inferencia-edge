use ringbuffer::RingBuffer;
use triton_proxy_lib::{
    policy::{Policy, Request},
    server::{Endpoint, Endpoints}
};
use uuid::Uuid;
use super::{process_locally, Model, SimpleContext};
use anyhow::Result;

#[derive(Default)]
/// Reglas:
/// - Si hay algun endpoint que no se ha usado nunca, escoge ese.
/// - Si no, escoge que tenga una latencia media (5 ultimos intentos) más baja.
///
/// **Problema**: si localhost se considera óptim y le spameo peticiones, 
/// localhost se va a sobrecargar, y se empezará a enviar a otro vecino.
/// Pero cuando localhost vuelve a la normalidad, ya no se le vuelven a enviar
/// peticiones.
pub struct MinLatencia {
    models: Vec<Model>
}

impl Policy<SimpleContext> for MinLatencia {

    async fn choose_target(&self, request: &Request<SimpleContext>, endpts: &Endpoints<SimpleContext>) -> Uuid {

        log::info!("MIERDA: {:?}", request.previous_nodes);
        let node_uuid = endpts.iter()
            // Para no hacer un ciclo.
            .filter(|ep| !request.previous_nodes.contains(ep.0))
            .min_by_key(|(_, ep)| calcular_peso(ep))
            .map(|(uuid, _)| *uuid)
            .unwrap();

       
       node_uuid
    }

    async fn process_locally(&self, request: &Request<SimpleContext>) -> Result<Vec<u8>> {
        let model = self.choose_model(request);
        process_locally(request, model).await
    }
}

impl<'a> MinLatencia {
     fn choose_model(&'a self, _request: &Request<SimpleContext>) -> &'a Model {
        self.models.iter()
            .max_by_key(|model| model.perf)
            .unwrap()
     }

}

/// Devuelve 0 si no se ha usado nunca.
fn calcular_peso(endp: &Endpoint<SimpleContext>) -> u32 {
    
    let numero_endps = endp.last_results.len() as u32;
    let sum_latencia: u32 = endp.last_results.iter()
        .map(|res| {
            match res {
                Ok(res) => res.duration.as_millis() as u32,
                Err(_) => 10 * 1000 // 10 segundos si hubo fallo.
            }
        })
        .sum();
     
    let promedio = sum_latencia.checked_div(numero_endps).unwrap_or(0);
    log::info!("Promedio latencias pod {}: {}", endp.name, promedio);
    promedio
}
