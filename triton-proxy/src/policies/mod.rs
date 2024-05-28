pub mod min_latencia;
pub mod min_queue;
pub mod requisitos;

use std::fmt::Debug;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::{model::Model, server::Endpoints};

pub use min_latencia::MinLatencia;
pub use requisitos::Requisitos;

#[derive(Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: Uuid,
    pub jumps: u32,
    pub priority: u8,
    pub accuracy: u8,
    pub content: Vec<u8>,
    pub previous_nodes: Vec<Uuid>
}

impl Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Request {{ id: {}, jumps: {}, priority: {}, accuracy: {}, im_size: {} }}",
            self.id,
            self.jumps,
            self.priority,
            self.accuracy,
            self.content.len()
        )
    }
}

pub trait Policy: Default + Send + Sync {
    fn choose_target(&self, request: &Request, endpoints: &Endpoints) -> impl std::future::Future<Output = Uuid> + std::marker::Send;
    fn choose_model<'a>(&self, request: &Request, models: &'a [Model]) -> &'a Model;
}

#[derive(Default)]
pub struct Random;

impl Policy for Random {
    async fn choose_target(&self, _request: &Request, endpoints: &Endpoints) -> Uuid {

        let read_handle = endpoints;
        let n_nodes = read_handle.len();
        let node_index = rand::random::<usize>() % n_nodes;
        let node_uuid = read_handle.keys().nth(node_index).cloned().unwrap();
        node_uuid 
    }

    fn choose_model<'a>(&self, _request: &Request, models: &'a [Model]) -> &'a Model {
        let model_index = rand::random::<usize>() % models.len();
        &models[model_index]
    }
}

