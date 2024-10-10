use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use itertools::Itertools;
use tokio::sync::Mutex;
use edge_proxy_lib::{policy::{Policy, Request}, server::Endpoints};
use uuid::Uuid;
use super::{process_locally, read_models, Model, SimpleContext, TritonEndpoints, TritonRequest};

#[derive(Debug, Default)]
pub struct Rrobin {
    pod_name: String,
    models: Vec<Model>,
    siguiente: AtomicUsize,
}

impl Rrobin {
    pub fn new(path: &str, pod_name: String) -> Result<Self> {
        Ok(Self {
            pod_name, 
            siguiente: AtomicUsize::new(0),
            models: read_models(path)?,
        })
    }
}

impl Policy<SimpleContext> for Rrobin {
    async fn choose_target(&self, request: &TritonRequest, endpoints: &TritonEndpoints) -> Uuid {
        
        let siguiente = self.siguiente.fetch_add(1, Ordering::Relaxed);
        log::info!("rrobin: siguiente={}", siguiente);
        let n_endps = endpoints.len();
        *endpoints.iter()
            .nth(siguiente % n_endps)
            .unwrap().0
    }

    async fn process_locally(&self, request: &TritonRequest) -> Result<Vec<u8>> {

        let model = self.models.iter()
            .min_by_key(|m| m.perf)
            .unwrap();

        process_locally(&request, model).await
    }
}
