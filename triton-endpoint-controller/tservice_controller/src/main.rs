mod policies;

use kube::{Config, Client};
use log::{info, error};
use anyhow::{anyhow, Result};

type POLICY = policies::HwOnly;

#[tokio::main]
async fn main() -> Result<()> {

    if cfg!(debug_assertions) {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
            .init();
        console_subscriber::init();
    }
    else { env_logger::init(); }

    let cluster_config = Config::incluster();
    let client = match cluster_config {
        Ok(config) => {
            info!("Running triton_proxy inside cluster.");
            Client::try_from(config)
        }
        Err(_) => {
            info!("Running triton_proxy outside cluster.");
            Client::try_default().await
        }
    };    

    match client {
        
        Ok(client) => {
            
            tservice_lib::run::<POLICY>(client, "./graph.json");
            tokio::signal::ctrl_c().await.unwrap();
            Ok(())
        },

        Err(e) => {
            error!("Failed to start Kubernetes client.");
            Err(anyhow!(e))
        }
    }
}

