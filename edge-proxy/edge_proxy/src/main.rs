mod policies;
mod utils;

use std::{env, process::Command};
use kube::{Client, Config};
use log::{error, info};
use anyhow::{anyhow, Context, Result};
use policies::{MinLatencia, SimpleContext};
use edge_proxy_lib::metrics::Metric;
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
            info!("Running edge_proxy inside cluster.");
            Client::try_from(config)
        }
        Err(_) => {
            info!("Running edge_proxy outside cluster.");
            Client::try_default().await
        }
    };

    if let Err(e) = start_triton_client() {
        error!("Failed to start Python triton client: {e}");
        return Err(e);
    }

    let (pod_namespace, pod_name, pod_uuid) = get_env_vars()?;
    let policy = MinLatencia::new(CSV_MODELOS)?;
    let metrics = get_target_metrics();
    match client {
        Ok(client) => {
            edge_proxy_lib::main_task::<MinLatencia, SimpleContext>(
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

fn start_triton_client() -> anyhow::Result<()> {

    // Arrancar el cliente triton a la esperea de peticiones.
    Command::new("python3")
        .arg("-u")
        .arg("./cliente.py")
        .arg("-u")
        .arg("127.0.0.1:8000") //TODO: cuidado con esto!!
        .spawn()?;

    Ok(())
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
