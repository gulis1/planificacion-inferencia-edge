use std::fmt::Debug;
//use serde::{Deserialize, Serialize};
use tokio::{
    io::{BufReader, BufWriter},
    net::TcpStream
};
use uuid::Uuid;
use crate::server::Endpoints;
use anyhow::Result;

pub struct Sender<'a> {
    pub s: BufWriter<&'a mut TcpStream>
}

// TODO: safety
unsafe impl<'a> Sync for Sender<'a> {}

pub struct Receiver<'a> {
    pub r: BufReader<&'a mut TcpStream>
}

// TODO: safety
unsafe impl<'a> Sync for Receiver<'a> {}

pub trait RequestContext: Send + Debug + Clone  + Sync {

    fn receive(reader: &mut Receiver) -> impl std::future::Future<Output = Result<Self, std::io::Error>> + Send + Sync;
    fn send(sender: &mut Sender, req: &Self) -> impl std::future::Future<Output = Result<(), std::io::Error>> + Send + Sync;
}

//#[derive(Clone, Serialize, Deserialize)]
pub struct Request<R: RequestContext> {
    pub id: Uuid,
    pub jumps: u32,
    pub context: R,
    pub content: Vec<u8>,
    pub previous_nodes: Vec<Uuid>
}

impl<R: RequestContext> Debug for Request<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Request {{ id: {}, jumps: {}, context: {:?}, im_size: {} }}",
            self.id,
            self.jumps,
            self.context,
            self.content.len()
        )
    }
}

pub trait Policy<R: RequestContext>: Send + Sync {
    fn choose_target(&self, request: &Request<R>, endpoints: &Endpoints<R>) -> impl std::future::Future<Output = Uuid> + std::marker::Send;
    fn process_locally(&self, request: &Request<R>) -> impl std::future::Future<Output = Result<Vec<u8>>> + std::marker::Send;
}
