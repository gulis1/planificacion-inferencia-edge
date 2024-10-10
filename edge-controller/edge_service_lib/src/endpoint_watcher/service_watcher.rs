use super::Message;

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use anyhow::{Context, Result};
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::Metadata;
use kube::api::{ObjectMeta, PartialObjectMetaExt, Patch, PatchParams};
use kube::runtime::{watcher, WatchStreamExt};
use kube::{Api, Client, ResourceExt};
use log::{error, info};
use petgraph::Direction;
use petgraph::dot::{Config, Dot};
use serde::Serialize;
use tokio::{sync::mpsc, task::JoinHandle};
use petgraph::graphmap::DiGraphMap;
use crate::policy::{GraphWrapper, Policy};
use uuid::Uuid;

const LABEL_NAME: &str = "edgeservices.prueba.ucm.es";
const ANNOT_NAME: &str = "edgeservices.prueba.ucm.es/endpoints";

#[derive(Clone, Debug, Serialize)]
pub struct Neighbor {
    uuid: Uuid,
    name: String,
    ip: String,
}

#[derive(Debug)]
pub struct ServiceWatcher<T: Policy> {
    pod_graph: DiGraphMap<Uuid, ()>,
    pods: BTreeMap<Uuid, Pod>,
    api: Arc<Api<Pod>>,
    watcher_handle: JoinHandle<Result<(), watcher::Error>>  ,
    policy: T
}

impl<T: Policy> Drop for ServiceWatcher<T> {
    fn drop(&mut self) {
        self.watcher_handle.abort();
        info!("Stopped watcher for deleted service.");
    }
}

type MsgSender = mpsc::Sender<Message>;
impl<T: Policy> ServiceWatcher<T> {

    pub async fn new(
        service_uid: Uuid,
        client: Client, 
        msg_sender: MsgSender, 
        namespace: &str, 
        selector: String) -> Self 
    {
        let watcher_handle = start_watcher(service_uid, client.clone(), namespace, selector, msg_sender);
        Self {
            pod_graph: DiGraphMap::new(),
            pods: BTreeMap::new(),
            api: Arc::new(Api::namespaced(client, namespace)),
            watcher_handle,
            policy: T::default().await
        }
    }

    /// Returns error if UID is not valid.
    pub fn add_pod(&mut self, pod: Pod) -> Result<()> {

        let uid = Uuid::parse_str(&pod.metadata()
            .uid.as_ref()
            .context("Pod missing UID")?
        )?;

        let affected = if !self.pods.contains_key(&uid) {
            self.pods.insert(uid, pod);
            self.pod_graph.add_node(uid);
            let mut wrapper = GraphWrapper::new(&mut self.pod_graph);
            let mut affected = self.policy.pod_added(&mut wrapper, &self.pods, uid);
            affected.push(uid);
            affected
        }
        else {
            // Replace pod with updated values.
            self.pods.insert(uid, pod);
            let mut wrapper = GraphWrapper::new(&mut self.pod_graph);
            self.policy.pod_updated(&mut wrapper, &self.pods, uid)
        };
        
        self.notify_pods(&affected);
        Ok(())
    }

    /// Returns error if UID is not valid.
    pub fn remove_pod(&mut self, pod: Pod) -> Result<()> {
        
        let uid = Uuid::parse_str(pod.metadata()
            .uid.as_ref()
            .context("Pod missing UID")?
        )?;

        if self.pods.contains_key(&uid) {
            self.pods.remove(&uid);
            let incoming: Vec<Uuid> = self.pod_graph
                .edges_directed(uid, Direction::Incoming)
                .map(|edge| edge.0)
                .collect();
            
            // Remove the node and notify all pods who had connections to it. 
            self.pod_graph.remove_node(uid);
            self.notify_pods(&incoming); 
            
            let mut wrapper = GraphWrapper::new(&mut self.pod_graph);
            let affected = self.policy.pod_removed(&mut wrapper, &self.pods, uid, &incoming);
            self.notify_pods(&affected);
        }

        Ok(())
    }

    fn notify_pods(&self, pods: &[Uuid]) {
        for pod in pods {
            self.notify_pod(&self.pods.get(&pod).unwrap());
        }
    }

    fn notify_pod(&self, pod: &Pod) {
        
        let pod_uuid = Uuid::from_str(&pod.metadata.uid.as_ref().unwrap()).unwrap();
        let mut neighbors: Vec<Neighbor> = self.pod_graph.neighbors_directed(pod_uuid, Direction::Outgoing)
            .flat_map(|uid| {

                let pod = self.pods.get(&uid).unwrap();
                let uuid = Uuid::parse_str(&pod.metadata.uid.as_ref()?).ok()?;
                let ip = pod.status.as_ref()?.pod_ip.clone()?;
                Some(Neighbor {
                    name: pod.name_any(),
                    uuid,
                    ip,
                })
            })
            .collect();
        
        // Añadirse a si mismo como vecino.
        neighbors.push(Neighbor {
            uuid: pod_uuid,
            name: pod.name_any(),
            ip: pod.status.as_ref().unwrap().pod_ip.clone().unwrap()
        });
        let neighbor_string = serde_json::to_string_pretty(&neighbors).unwrap(); 
        let api = Arc::clone(&self.api);
        set_annotation(pod, api, ANNOT_NAME, neighbor_string);
    }

    pub fn export_graph(&self) -> String {
        let dot = Dot::with_config(&self.pod_graph, &[Config::EdgeNoLabel]);
        format!("{:?}", dot)
    }
}

fn start_watcher(service_uid: Uuid, client: Client, namespace: &str, selector: String, sender: MsgSender) ->
    JoinHandle<Result<(), watcher::Error>>
{
    let api = Api::<Pod>::namespaced(client, namespace);
    tokio::spawn(async move {

        let label = format!("{LABEL_NAME}={selector}");
        let watch_config = watcher::Config::default()
            .labels(&label);

        let sender = &sender;
        info!("Starting watcher for pods with label: {label}");
        let result = watcher(api, watch_config).touched_objects()
            .into_stream()
            .try_for_each(|pod| async move {
                on_pod_update(pod, sender, service_uid).await
            })
            .await;

        match result {
            Ok(_) => info!("Watcher for pods with label {label} ended succesfully."),
            Err(ref e) => error!("Watcher for pods with label {label} ended with error {e}")   
        };
        
        result
    })
}

async fn on_pod_update(pod: Pod, sender: &MsgSender, service_uid: Uuid) -> Result<(), watcher::Error> {
    
    let is_ready = pod_readiness(&pod).unwrap_or(false);
    match is_ready {
        true => sender.send(Message::PodReady { service_uid, pod }),
        false => sender.send(Message::PodUnready { service_uid, pod}),
    }
    .await
    .expect("Failed to send message");

    Ok(())
}

fn pod_readiness(pod: &Pod) -> Option<bool> {

    let readiness = pod.status.as_ref()?
        .conditions.as_ref()?
        .iter()
        .find(|cond| cond.type_ == "Ready")
        .map(|cond| &cond.status == "True")?;

    Some(readiness)
}

// Adds or changes an annotation for a pod
fn set_annotation(pod: &Pod, api: Arc<Api<Pod>>, annotation_name: &str, annotation_value: String) {
    let pod_name = pod.name_any();
    let mut annotations = pod.annotations().clone();
    let annotation_name = annotation_name.to_string();
    tokio::spawn(async move {
        annotations.insert(annotation_name, annotation_value);

        let meta = ObjectMeta {
            annotations: Some(annotations),
            ..Default::default()
        }.into_request_partial::<Pod>();

        let err = api.patch_metadata(
            &pod_name,
            &PatchParams::apply("tservice-controller"),
            &Patch::Apply(meta)
        )
        .await;

        match err {
            Ok(_) => info!("Pod patch applied succesfully."),
            Err(e) => error!("Error en apply: {e}")
        }
    });
}
