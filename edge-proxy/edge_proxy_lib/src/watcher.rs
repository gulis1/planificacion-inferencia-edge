use std::collections::BTreeMap;
use std::sync::Arc;
use anyhow::{Result, Context};
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{ObjectMeta, PartialObjectMetaExt, Patch, PatchParams};
use kube::runtime::{metadata_watcher, watcher, WatchStreamExt};
use kube::{Api, Client, ResourceExt};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use crate::{Message, ENDPS_ANNOT};
use serde_json::Value as JsonValue;


pub struct AnnotationsWatcher {
    pod_name: Arc<str>,
    api: Arc<Api<Pod>>,
    last_annotations: Arc<Mutex<BTreeMap<String, String>>>,
    task_handle: Option<JoinHandle<()>>,
}

impl Drop for AnnotationsWatcher {
    fn drop(&mut self) {
        if let Some(handle) = &self.task_handle {
            handle.abort();
        }
    }
}

impl AnnotationsWatcher {

    pub fn new(
        client: Client,
        pod_name: &str,
        namespace: &str,
        sender: mpsc::Sender<Message<'static>>
    ) -> Self 
    {
        let api: Api<Pod> = Api::namespaced(client, &namespace);
        let mut watcher = Self {
            pod_name: Arc::from(pod_name),
            api: Arc::new(api),
            last_annotations: Arc::new(Mutex::new(BTreeMap::default())),
            task_handle: None,
        };

        watcher.run(sender);
        watcher
    }

    fn run(&mut self, sender: mpsc::Sender<Message<'static>>) {

        // Watcher task
        let api = self.api.as_ref().clone();
        let pod_name = self.pod_name.clone();
        let last_annotations = Arc::clone(&self.last_annotations);
        let handle = tokio::spawn(async move {
        
            let watch_config = watcher::Config::default()
                .fields(&format!("metadata.name={pod_name}"));

            let last_annotations = &last_annotations;
            let sender = &sender;
            let _ = metadata_watcher(api, watch_config).touched_objects()
                .into_stream()
                .try_for_each(|pod| async move {
                    
                    if let Some(annots) = pod.metadata.annotations {     
                        
                        let mut previous_annots = last_annotations.lock().await;
                        if let Some(endpoints) = annots.get(ENDPS_ANNOT) {
                            
                            // If endpoints have changed
                            if !previous_annots.get(ENDPS_ANNOT).is_some_and(|prev_endps| *prev_endps == *endpoints) {
                                
                                match serde_json::from_str(&endpoints) {
                                    Ok(new_endpoints) => {
                                        sender.send(Message::EndpointsChanged(new_endpoints)).await.unwrap();
                                        log::info!("Received new endpoints: {:?}", endpoints);
                                    }
                                    Err(e) => log::error!("Failed to parse received endpoints: {e}")
                                }
                            }         
                        }
                        *previous_annots = annots;
                    }
                    
                    Ok(())
                })
                .await;
                
                // El watcher no debería terminar nunca.
                log::error!("Pod watcher exited.");      
        });

        self.task_handle = Some(handle);
    }

    pub async fn add_annot<T>(&self, annots: Vec<(&str, T)>)
        where T: ToString
    {
        let mut annotations_guard = self.last_annotations.lock().await;
        
        for annot in annots {
            annotations_guard.insert(annot.0.to_string(), annot.1.to_string());
        }
        
        // Clone the annotations to send them and drop the mutex.
        let annotations = (*annotations_guard).clone();
        drop(annotations_guard);
        let meta = ObjectMeta {
            annotations: Some(annotations),
            ..Default::default()
        }.into_request_partial::<Pod>();

        let err = self.api.patch_metadata(
            &self.pod_name,
            &PatchParams::apply("tservice-controller"),
            &Patch::Apply(meta)
        )
        .await;

        if let Err(e) = err { log::error!("Error applying patch: {e}"); }
    }

    pub fn get_another_pods_metrics(&self, pod_name: Arc<str>, annot_name: String, respond_to: oneshot::Sender<Option<JsonValue>>) {
        let api = Arc::clone(&self.api);
        tokio::spawn(async move {
            
            log::info!("Querying annot {annot_name} for {pod_name}");
            let result: Result<JsonValue> = api.get(&pod_name).await.context("Failed to query pod")
                .and_then(|pod| {
                    let x = pod.annotations()
                        .get(&annot_name)
                        .cloned();

                    x.with_context(|| format!("Pod {pod_name} missing {annot_name} annot"))
                })
                .and_then(|text| serde_json::from_str(text.as_ref()).map_err(anyhow::Error::from));
            
            match result {
                Ok(payload) => {
                    log::info!("Succesfully queryed annot {annot_name} for {pod_name}");
                    respond_to.send(Some(payload))
                },
                Err(e) => {
                    log::error!("Could not query annot {annot_name} for {pod_name}: {e}");
                    respond_to.send(None)
                }
            }.expect("Failed to send oneshot message.");
        });
    }

}
