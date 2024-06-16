mod policies;
mod utils;

use std::env;
use kube::{Client, Config};
use log::{error, info};
use anyhow::{anyhow, Context, Result};
use policies::{Requisitos, SimpleContext};
use triton_proxy_lib::metrics::Metric;
use uuid::Uuid;

const CSV_MODELOS: &str = "./modelos.csv";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {

    if cfg!(debug_assertions) {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .init();
    }
    else { env_logger::init(); }
    
    let cluster_config = Config::incluster();
    let client = match cluster_config {
        Ok(config) => {
            info!("Running triton_proxy inside cluster.");
            Client::try_from(config)
        }
        Err(_) => {
            info!("Running triton_proxy outside cluster.");
            Client::try_default().await
        }
    };

    let (pod_namespace, pod_name, pod_uuid) = get_env_vars()?;
    let policy = Requisitos::new(CSV_MODELOS)?;
    let metrics = get_target_metrics();
    match client {
        Ok(client) => {
            triton_proxy_lib::main_task::<Requisitos, SimpleContext>(
                client,
                pod_namespace,
                pod_name,
                pod_uuid,
                policy,
                metrics
            ).await
        },
        Err(e) => {
            error!("Failed to start Kubernetes api client: {e}");
            Err(anyhow!("Failed to start Kubernetes client"))
        }
    }
}

fn get_target_metrics() -> Vec<Metric> {
    
    let metrics = [
        (   
            "queue_avg_5m",
            "sum(increase(nv_inference_queue_duration_us[5m])) / sum(increase(nv_inference_exec_count[5m]))"
        ),
        (
            "total_inferences",
            "sum(nv_inference_exec_count)"
        ),
        (
            "pending_requests",
            "sum(nv_inference_pending_request_count)"
        )
    ];

    metrics.into_iter()
        .map(|(name, query)| Metric::new(name, query))
        .collect()
}

fn get_env_vars() -> Result<(String, String, Uuid)> {
    
    let pod_namespace = env::var("POD_NAMESPACE").context("Missing POD_NAMESPACE env variable.")?;
    let pod_name = env::var("POD_NAME").context("Missing POD_NAME env variable.")?;
    let pod_uuid = env::var("POD_UUID").context("Missing POD_UUID env variable.")?;
    let pod_uuid = Uuid::parse_str(&pod_uuid)?;
    info!("Retrieved env variables: POD_NAMESPACE={pod_namespace}, POD_NAME={pod_name}, POD_UUID={pod_uuid}");
    Ok((pod_namespace, pod_name, pod_uuid))
}
