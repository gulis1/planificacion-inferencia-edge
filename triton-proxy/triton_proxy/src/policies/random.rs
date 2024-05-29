use rand::random;
use triton_proxy_lib::{
    policy::{Policy, Request, RequestContext},
    server::Endpoints
};
use uuid::Uuid;
use super::{process_locally, read_models, Model};
use anyhow::Result;

#[derive(Default)]
pub struct Random {
    models: Vec<Model>
}

impl Random {
    pub fn new(path: &str) -> Result<Self> {
        Ok(Self {
            models: read_models(path)?
        })
    }
}

impl<R: RequestContext> Policy<R> for Random {
     
    async fn choose_target(&self, _request: &Request<R>, endpoints: &Endpoints) -> Uuid {

        let read_handle = endpoints;
        let n_nodes = read_handle.len();
        let node_index = rand::random::<usize>() % n_nodes;
        let node_uuid = read_handle.keys().nth(node_index).cloned().unwrap();
        node_uuid 
    }

    async fn process_locally(&self, request: &Request<R>) -> Result<Vec<u8>> {
        let model = &self.models[random::<usize>() % self.models.len()];
        process_locally(request, model).await
    }
}
