mod watcher;
mod server;
mod hardware;
mod metrics;
mod policies;
mod model;

use std::{env, sync::Arc};
use hardware::get_hardware_info;
use kube::{Client, Config};
use log::{error, info};
use anyhow::{anyhow, Context, Result};
use metrics::PrometheusClient;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;
use watcher::AnnotationsWatcher;
use serde_json::Value as JsonValue;
use crate::{
    policies::{min_queue::MinQueue, MinLatencia},
    server::ProxyServer
};

pub const MODEL_CSV_PATH: &str = "./modelos.csv";
pub const METRICS_ANNOT: &str = "tritonservices.prueba.ucm.es/triton_metrics";
pub const HW_ANNOT: &str =      "tritonservices.prueba.ucm.es/hw_info";
pub const ENDPS_ANNOT: &str =   "tritonservices.prueba.ucm.es/endpoints";

type MsgSender<'a> = mpsc::Sender<Message<'a>>;
#[derive(Debug)]
pub enum Message<'a> {
    EndpointsChanged(JsonValue),
    AnnotationUpdate(Vec<(&'a str, String)>),
    NeighborAnnotRequest { neighbor_name: Arc<str>, annot_name: String, respond_to: oneshot::Sender<Option<JsonValue>>}
}

#[tokio::main]
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
    match client {
        Ok(client) => main_task(client, pod_namespace, pod_name, pod_uuid).await,
        Err(e) => {
            error!("Failed to start Kubernetes api client: {e}");
            Err(anyhow!("Failed to start Kubernetes client"))
        }
    }
}

async fn main_task(
    client: Client, 
    pod_namespace: String, 
    pod_name: String,
    pod_uuid: Uuid
) -> Result<()>
{
    let hw_info = get_hardware_info(); 
    info!("Detected hardware: {:?}", hw_info);

    let (sender, mut receiver) = tokio::sync::mpsc::channel::<Message>(32);
    let proxy_server = ProxyServer::<MinQueue>::new(
        pod_uuid,
        "0.0.0.0:9999",
        sender.clone(),
        MODEL_CSV_PATH,
        hw_info.clone()
    ).await?;

    let watcher = AnnotationsWatcher::new(
        client,
        &pod_name,
        &pod_namespace,
        sender.clone()
    );

    let _metrics_client = PrometheusClient::new("http://localhost:9090",
        sender.clone())?;

    // Update the server with the probed hardware.
    watcher.add_annot(vec![(HW_ANNOT, hw_info)]).await;
    loop {
        
        let message = receiver.recv().await.context("Message channel closed.")?;
        log::debug!("New message received: {:?}", message);
        match message {
            Message::EndpointsChanged(endpoints) => {
                proxy_server.update_endpoints(endpoints);
            },
            Message::AnnotationUpdate(annots) => {
                watcher.add_annot(annots).await;
            }
            Message::NeighborAnnotRequest { neighbor_name, annot_name, respond_to } => {
                watcher.get_another_pods_metrics(neighbor_name, annot_name, respond_to)
            }
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
