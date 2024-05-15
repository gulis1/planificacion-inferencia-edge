use std::{path::Path, time::Duration};

use endpoint_watcher::Message;
use kube::Client;
use policy::Policy;
use tokio::{sync::mpsc::Sender, time::sleep};

mod controller;
mod endpoint_watcher;
pub mod policy;

pub fn run<T: Policy>(client: Client, initial_grap_file: &str) {

    let msg_sender = endpoint_watcher::run::<T>(client.clone()); 
    controller::run(client.clone(), msg_sender.clone());
    check_for_graph_file(initial_grap_file, &msg_sender);
}

fn check_for_graph_file(path: &str, sender: &Sender<Message>) {

    let path = Path::new(path);
    if !path.exists() { log::warn!("File ./graph.json not found."); }
    else {
        log::info!("Found graph.json file.");
        let sender = sender.clone();
        let path = path.to_owned();
        tokio::spawn(async move {
            sleep(Duration::from_secs(5)).await;
            sender.send(Message::LoadGraphFromFile { file: path })
                .await
                .expect("Failed to load graph file: could not send message.");
        });
    }
}
