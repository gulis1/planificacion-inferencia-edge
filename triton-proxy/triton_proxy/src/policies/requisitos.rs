use ringbuffer::RingBuffer;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use triton_proxy_lib::{
    policy::{Policy, Receiver, Request, RequestContext, Sender},
    server::{Endpoint, Endpoints}
};
use uuid::Uuid;
use anyhow::Result;

use super::SimpleContext;


#[derive(Debug, Clone, Default)]
pub struct Requisitos;

impl Policy<SimpleContext> for Requisitos {
    async fn choose_target(&self, request: &Request<SimpleContext>, endps: &Endpoints) -> Uuid {
        // Quitar nodos anteriores para que no haya ciclos.
        let nodes: Vec<(&Uuid, &Endpoint)> = endps.iter()
            .filter(|(uuid, _)| !request.previous_nodes.contains(uuid))
            .collect();

        nodes[0].0.clone()
    }
    
    async fn process_locally(&self, request: &Request<SimpleContext>) -> Result<Vec<u8>> {
        todo!()
    }
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
