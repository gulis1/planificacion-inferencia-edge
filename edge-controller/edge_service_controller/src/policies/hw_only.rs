use std::collections::HashSet;

use kube::ResourceExt;
use edge_service_lib::policy::*;
use serde_json::{json, Value as JsonValue};
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct HwOnly {
    already_added: HashSet<Uuid>
}

impl Policy for HwOnly {
    fn pod_added(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: uuid::Uuid) -> Vec<uuid::Uuid> {
        log::info!("Pod added: {pod}");
        if let Some(hw_info) = get_hw_info(pods, &pod) {
            self.already_added.insert(pod);
            let tmp = serde_json::to_string_pretty(&hw_info).unwrap();
            log::info!("hw_info for pod {pod}:\n{tmp}");
        }
        else {
            log::warn!("Pod {pod} missing hw_info.");
        }

        Vec::new()
    }

    fn pod_removed(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: uuid::Uuid, affected: &[uuid::Uuid]) -> Vec<uuid::Uuid> {
        log::info!("Pod removed: {pod}");
        Vec::new()
    }

    fn pod_updated(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: uuid::Uuid) -> Vec<uuid::Uuid> {
        if !self.already_added.contains(&pod) {
            log::info!("Pod updated: {pod}");
        }
        Vec::new()
    }
}

impl AsyncDefault for HwOnly {
    async fn default() -> Self {
        Self { already_added: HashSet::new() }
    }
}

#[inline]
fn get_hw_info(pods: &PodMap, pod: &Uuid) -> Option<JsonValue> {
    
    let hw_info = pods.get(pod)?
        .annotations()
        .get("edgeservices.prueba.ucm.es/hw_info")?;

    match serde_json::from_str(hw_info.as_str()) {
        Ok(hw_info) => Some(hw_info),
        Err(e) => {
            log::error!("Failed to parse hw_info for pod {pod}: {e}");
            None
        }
    }
}
