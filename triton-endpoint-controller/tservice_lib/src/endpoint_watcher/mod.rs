mod service_watcher;

use std::{collections::HashMap, str::FromStr}; 
use k8s_openapi::api::core::v1::Pod;
use log::{debug, info};
use kube::Client;

use tokio::sync::{mpsc, oneshot};
use service_watcher::ServiceWatcher;
use crate::policy::Policy;
use uuid::Uuid;
use anyhow::Context;

const CHANNEL_SIZE: usize = 128;

#[derive(Debug)]
pub enum Message {
    NewService { service_uid: Uuid, namespace: String, selector: String },
    DeleteService  { service_uid: Uuid },
    PodReady { service_uid: Uuid, pod: Pod },
    PodUnready { service_uid: Uuid, pod: Pod },
    ExportGraph { service_uid: Uuid, response_to: oneshot::Sender<String> }
}

pub fn run<T: Policy>(client: Client) -> mpsc::Sender<Message> {

    let (sender, mut receiver) = mpsc::channel(CHANNEL_SIZE);
    let msg_sender = sender.clone();
    tokio::spawn(async move {
        
        let mut service_watchers: HashMap<Uuid, ServiceWatcher<T>> = HashMap::new();
        loop {
            let msg = receiver.recv().await.expect("Channel closed.");
            match msg {
                Message::NewService { service_uid, namespace, selector } => {
                    if !service_watchers.contains_key(&service_uid) {
                        let service = ServiceWatcher::new(service_uid, client.clone(), msg_sender.clone(), &namespace, selector).await;
                        info!("Adding watcher for service {service_uid}");
                        service_watchers.insert(service_uid, service);
                    }   
                },
                Message::DeleteService{service_uid}=> {
                    if service_watchers.contains_key(&service_uid) {
                        info!("Removing watcher for service {service_uid}");
                        service_watchers.remove(&service_uid);
                    }
                },
                Message::PodReady { service_uid, pod } => {
                    // TODO: este print sale detrás del de aplicar el parche????
                    info!("Received new pod for service {service_uid}");
                    if let Some(service) = service_watchers.get_mut(&service_uid) {
                        service.add_pod(pod).expect("AAA");
                    }
                },
                Message::PodUnready { service_uid, pod } => {
                    if let Some(service) = service_watchers.get_mut(&service_uid) {
                        info!("Deleted por for service {service_uid}");
                        service.remove_pod(pod).expect("AAA");
                    }
                },
                Message::ExportGraph { service_uid, response_to } => {
                    if let Some(service) = service_watchers.get(&service_uid) {
                        let graph_string = service.export_graph();
                        if let Err(_) = response_to.send(graph_string) {
                            log::error!("Failed to send graph export message.");
                        }
                    }
                }
            };

            debug!("Services: {:?}", service_watchers.values());
        }
    });
    
    let msg_sender = sender.clone();
    tokio::spawn(async move {
        graph_export_server(msg_sender).await;
    });

    sender
}

async fn graph_export_server(sender: mpsc::Sender<Message>) {

    let mut server = tide::new();
    server.at("/:path").get(move |request: tide::Request<()>| {

        let sender = sender.clone();
        async move {
        
            let uri = request.url().path_segments()
                .and_then(|url| url.last())
                .context("Invalid URL").unwrap().to_string();
            
            let uuid = Uuid::from_str(&uri)?;
            let (s, r) = oneshot::channel::<String>();
            sender.send(Message::ExportGraph {
                service_uid: uuid,
                response_to: s
            }).await?;

            Ok(r.await?)
        }
    });
    server.listen("0.0.0.0:9091").await.expect("HTTP server ended.");

}
