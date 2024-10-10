use k8s_openapi::serde::{Deserialize, Serialize};
use log::error;
use schemars::JsonSchema;
use kube::runtime::finalizer::Event as Finalizer;
use tokio::sync::mpsc;
use uuid::Uuid;
use std::{sync::Arc, time::Duration};
use kube::runtime::{finalizer, watcher};
use kube::runtime::{controller::Action, Controller};
use kube::{Api, Client, CustomResource};
use futures::StreamExt;
use thiserror::Error;
use crate::endpoint_watcher::Message;

const FINALIZER_NAME: &str = "edgeservice.prueba.ucm.es/deletion";

/// This provides a hook for generating the CRD yaml (in crdgen.rs)
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(kind = "EdgeService", group = "prueba.ucm.es", version = "v1", namespaced)]
#[kube(status = "EdgeServiceStatus", shortname = "eservice")]
pub struct EdgeNodeSpec {
    pub selector: String
}
/// The status object of `Document`
#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct EdgeServiceStatus {
    pub prueba: i32
}

#[derive(Error, Debug)]
enum Error {
    // #[error("Kube Error: {0}")]
    // KubeError(#[source] kube::Error),
    
    #[error("Finalizer Error: {0}")]
    FinalizerError(#[source] Box<kube::runtime::finalizer::Error<Error>>),
}
type Result<T, E = Error> = std::result::Result<T, E>;

// Context for our reconciler
pub struct Context {
    client: Client
}

fn error_policy(_doc: Arc<EdgeService>, error: &Error, _ctx: Arc<Context>) -> Action {
    error!("Error en reconcile: {}", error);
    Action::requeue(Duration::from_secs(5))
}

async fn reconcile(tservice: Arc<EdgeService>, ctx: Arc<Context>, sender: mpsc::Sender<Message>) -> Result<Action> {
    
    let tservices = Api::<EdgeService>::namespaced(ctx.client.clone(), tservice.metadata.namespace.as_ref().unwrap());
    finalizer(&tservices, FINALIZER_NAME, tservice.clone(), |event| async {
        
        let tsevice_uid = Uuid::parse_str(tservice.metadata.uid
            .as_ref()
            .expect("Missing UID")
        )
        .expect("Invalid tservice UID");
    
        let action = match event {
            Finalizer::Apply(tservice) => {
                sender.send(Message::NewService { 
                    service_uid: tsevice_uid,
                    namespace: tservice.metadata.namespace.clone().expect("Missing tservice namespace"),
                    selector: tservice.spec.selector.clone()
                })
                .await
                .expect("Failed to send message.");
                Action::requeue(Duration::from_secs(300))
            }
            Finalizer::Cleanup(_) => {
                sender.send(Message::DeleteService{service_uid: tsevice_uid})
                .await
                .expect("Failed to send message.");
                Action::await_change()
            }
        };

        Ok(action)
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

pub fn run(client: Client, sender: mpsc::Sender<Message>) {

    let tservices = Api::<EdgeService>::all(client.clone());
    let context = Arc::new(Context { client });

    tokio::spawn(async move {
        let sender = sender;
        Controller::new(tservices, watcher::Config::default().any_semantic())
            .shutdown_on_signal( )
            .run(|tservice, ctx| reconcile(tservice, ctx, sender.clone()), error_policy, context)
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .await;
    });
}
