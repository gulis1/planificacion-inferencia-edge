mod service_watcher;

use std::{collections::HashMap, path::PathBuf, str::FromStr}; 
use k8s_openapi::api::core::v1::Pod;
use log::{debug, info};
use kube::Client;
use petgraph::graphmap::DiGraphMap;
use serde_json::Value as JsonValue;
use tokio::{
    fs, 
    sync::{mpsc, oneshot}
};
use service_watcher::ServiceWatcher;
use crate::policy::Policy;
use uuid::Uuid;
use anyhow::{Context, Result};

const CHANNEL_SIZE: usize = 128;

#[derive(Debug)]
pub enum Message {
    NewService { service_uid: Uuid, namespace: String, selector: String },
    DeleteService  { service_uid: Uuid },
    PodReady { service_uid: Uuid, pod: Pod },
    PodUnready { service_uid: Uuid, pod: Pod },
    LoadGraphFromFile { file: PathBuf },
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
                        let service = ServiceWatcher::new(service_uid, client.clone(), msg_sender.clone(), &namespace, selector);
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
                Message::LoadGraphFromFile { file } => {
                    match parse_graph_file(&mut service_watchers, file).await {
                        Ok(_) => log::info!("Succesfully applied graph file."),
                        Err(e) => log::error!("Failed to parse graph file: {e}")
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

async fn parse_graph_file<T>(services: &mut HashMap<Uuid, ServiceWatcher<T>>, file: PathBuf) -> Result<()>
    where T: Policy
{

    let content = fs::read_to_string(file).await?;
    let parsed: JsonValue = serde_json::from_str(&content)?;
    
    let service_uid: Uuid = parsed.get("service_uuid")
        .context("Json missing key 'service_uuid'.")?
        .as_str()
        .context("Invalid value for key 'service_uuid'.")?
        .try_into()?;

    let dot_content = parsed.get("graph")
        .context("Json missing key 'graph'.")?
        .as_str().unwrap();

    let mut g: DiGraphMap<Uuid, ()> = DiGraphMap::new();
    let parser = rust_dot::parse_string(dot_content);
    
    for node in &parser.nodes {
        let uid = Uuid::from_str(node)?;
        g.add_node(uid);
    }

    for edge in &parser.edges {
        let src = Uuid::from_str(parser.nodes.get(edge.0).unwrap())?;
        let dst = Uuid::from_str(parser.nodes.get(edge.1).unwrap())?;

        g.add_edge(src, dst, ());
    }

    let service = services.get_mut(&service_uid)
        .with_context(|| format!("Service {service_uid} not found."))?; 
    service.set_graph(g)
}

