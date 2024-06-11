use kube::Client;
use policy::Policy;

mod controller;
mod endpoint_watcher;
pub mod policy;

pub fn run<T: Policy>(client: Client) {

    let msg_sender = endpoint_watcher::run::<T>(client.clone());
    controller::run(client.clone(), msg_sender.clone());
}
