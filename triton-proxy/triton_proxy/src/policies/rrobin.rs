use anyhow::Result;
use itertools::Itertools;
use tokio::sync::Mutex;
use triton_proxy_lib::{policy::{Policy, Request}, server::Endpoints};
use uuid::Uuid;
use super::{process_locally, read_models, Model, SimpleContext, TritonEndpoints, TritonRequest};

#[derive(Debug, Default)]
pub struct Rrobin {
    pod_name: String,
    models: Vec<Model>,
    siguiente: Mutex<usize>
}

impl Rrobin {
    pub fn new(path: &str, pod_name: String) -> Result<Self> {
        Ok(Self {
            pod_name, 
            siguiente: Mutex::new(0),
            models: read_models(path)?
        })
    }
}

impl Policy<SimpleContext> for Rrobin {
    async fn choose_target(&self, request: &TritonRequest, endpoints: &TritonEndpoints) -> Uuid {

        let mut siguiente = self.siguiente.lock().await;
        log::info!("rrobin: siguiente={}", *siguiente);
        let uuid = match request.jumps {
            0 => {
                if *siguiente % 7 == 0 {
                    log::info!("rrobin: origen %7");
                    // Enviar local
                    *endpoints.iter()
                        .filter(|(_, ep)| ep.name.as_ref() == self.pod_name.as_str())
                        .exactly_one()
                        .unwrap().0
                }
                else {
                    log::info!("rrobin: origen NO %7");
                    *endpoints.iter()
                        .filter(|(_, ep)| ep.name.as_ref() != self.pod_name.as_str())
                        .exactly_one()
                        .unwrap().0
                }
            },
            1 => {
                log::info!("rrobin: salto 1");
                let ind = *siguiente;
                let n_endps = endpoints.len();
                *endpoints.iter().nth(ind % n_endps).unwrap().0
            },
            _ => {
                log::info!("rrobin: salto > que 1");
                *endpoints.iter()
                    .filter(|(_, ep)| ep.name.as_ref() == self.pod_name.as_str())
                    .exactly_one()
                    .unwrap().0    
            }
        };

        *siguiente =  siguiente.wrapping_add(1);
        uuid
    }

    async fn process_locally(&self, request: &TritonRequest) -> Result<Vec<u8>> {

        let model = self.models.iter()
            .min_by_key(|m| m.perf)
            .unwrap();

        process_locally(&request, model).await
    }
}
