mod watcher;
mod metrics;
pub mod server;
pub mod policy;
pub mod hardware;

use std::sync::Arc;
use kube::Client;
use policy::{Policy, RequestContext};
use tokio::sync::{mpsc, oneshot};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use anyhow::{Result, Context};

use crate::{
    hardware::get_hardware_info,
    metrics::PrometheusClient,
    server::ProxyServer,
    watcher::AnnotationsWatcher
};

const METRICS_ANNOT: &str = "tritonservices.prueba.ucm.es/triton_metrics";
const HW_ANNOT: &str =      "tritonservices.prueba.ucm.es/hw_info";
const ENDPS_ANNOT: &str =   "tritonservices.prueba.ucm.es/endpoints";

type MsgSender<'a> = mpsc::Sender<Message<'a>>;
#[derive(Debug)]
enum Message<'a> {
    EndpointsChanged(JsonValue),
    AnnotationUpdate(Vec<(&'a str, String)>),
    NeighborAnnotRequest { neighbor_name: Arc<str>, annot_name: String, respond_to: oneshot::Sender<Option<JsonValue>>}
}

pub async fn main_task<P, R> (
    client: Client, 
    pod_namespace: String, 
    pod_name: String,
    pod_uuid: Uuid,
    policy: P
) -> Result<()>
where P: Policy<R> + 'static,
      R: RequestContext + 'static
{
    let hw_info = get_hardware_info(); 
    log::info!("Detected hardware: {:?}", hw_info);

    let (sender, mut receiver) = tokio::sync::mpsc::channel::<Message>(32);
    let proxy_server = ProxyServer::<P, R>::new(
        pod_uuid,
        "0.0.0.0:9999",
        sender.clone(),
        policy

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
