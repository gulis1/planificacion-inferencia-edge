#![allow(dead_code)]

use itertools::Itertools;
use ringbuffer::RingBuffer;
use triton_proxy_lib::{policy::RequestContext, server::Endpoint};
use uuid::Uuid;

pub enum Order {
    Ascending(usize),
    Descending(usize)
}

pub fn escoger_n<'a, I, F, R>(order: Order, endps: I, mut key: F) -> Vec<(Uuid, &'a Endpoint<R>)>
    where I: Iterator<Item = (&'a Uuid, &'a Endpoint<R>)>,
          F: FnMut(&Endpoint<R>) -> u64,
          R: RequestContext
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
pub fn promedio_latencia(endp: &Endpoint<impl RequestContext>) -> u64 {
    
    let numero_endps = endp.last_results.len() as u64;
    let sum_latencia: u64 = endp.last_results.iter()
        .map(|res| {
            match res {
                Ok(res) => res.duration.as_millis() as u64,
                Err(_) => 10 * 1000 // 10 segundos si hubo fallo.
            }
        })
        .sum();

    // Default 0, porque si no nunca se probaría el endpoint. 
    let promedio = sum_latencia.checked_div(numero_endps).unwrap_or(0);
    log::info!("Promedio latencias pod {}: {}", endp.name, promedio);
    promedio
}


pub fn calcular_hw(ep: &Endpoint<impl RequestContext>) -> u64 {
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
pub fn carga_trabajo(ep: &Endpoint<impl RequestContext>) -> u64 {
    let carga = ep.metrics
        .as_ref()
        .and_then(|metr| metr.get("queue_avg_5m"))
        .and_then(|v| v.as_f64())
        .map(|v| v as u64)
        .unwrap_or(u64::MAX);

    log::info!("Carga trabajo de {}: {}", ep.name, carga);
    carga
}
