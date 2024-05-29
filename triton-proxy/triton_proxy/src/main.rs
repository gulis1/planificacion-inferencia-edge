mod policies;

use std::env;
use kube::{Client, Config};
use log::{error, info};
use anyhow::{anyhow, Context, Result};
use policies::{min_queue::MinQueue, SimpleContext};
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
    let policy = MinQueue::new(CSV_MODELOS)?;
    match client {
        Ok(client) => triton_proxy_lib::main_task::<MinQueue, SimpleContext>(client, pod_namespace, pod_name, pod_uuid, policy).await,
        Err(e) => {
            error!("Failed to start Kubernetes api client: {e}");
            Err(anyhow!("Failed to start Kubernetes client"))
        }
    }

}

fn get_env_vars() -> Result<(String, String, Uuid)> {
    
    let pod_namespace = env::var("POD_NAMESPACE").context("Missing POD_NAMESPACE env variable.")?;
    let pod_name = env::var("POD_NAME").context("Missing POD_NAME env variable.")?;
    let pod_uuid = env::var("POD_UUID").context("Missing POD_UUID env variable.")?;
    let pod_uuid = Uuid::parse_str(&pod_uuid)?;
    info!("Retrieved env variables: POD_NAMESPACE={pod_namespace}, POD_NAME={pod_name}, POD_UUID={pod_uuid}");
    Ok((pod_namespace, pod_name, pod_uuid))
}
