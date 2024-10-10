use std::{collections::HashMap, env, str::FromStr, sync::Arc, time::Duration};
use prometheus_http_query::{response::PromqlResult, Client};
use tokio::{sync::mpsc, task::{JoinHandle, JoinSet}, time::interval};
use anyhow::Result;
use crate::{Message, METRICS_ANNOT};

const DEFAULT_QUERY_INERVAL_SECS: u64 = 60;

pub(crate) struct PrometheusClient {
    task_handle: Option<JoinHandle<()>>
}

impl Drop for PrometheusClient {
    fn drop(&mut self) {
        if let Some(handle) = &self.task_handle {
            handle.abort();
        }
    }
}

#[derive(Clone, Debug)]
pub struct Metric {
    pub name: Arc<str>,
    pub query: Arc<str>
}

impl Metric {

    pub fn new(name: &str, query: &str) -> Self {
        Self {
            name: Arc::from(name),
            query: Arc::from(query)
        }
    }
}

impl PrometheusClient {

    pub fn new(server: &str, target_metrics: Vec<Metric>, sender: mpsc::Sender<Message<'static>>) -> Result<Self> {

        let mut client = Self {
            task_handle: None
        };

        client.run(server, target_metrics, sender)?;
        Ok(client)
    }

    fn run(&mut self, server: &str, target_metrics: Vec<Metric>, sender: mpsc::Sender<Message<'static>>) -> Result<()> {
    
        let client = Arc::new(Client::from_str(server)?);
        let interval_secs: u64 = env::var("METRICS_QUERY_INTERVAL_SECS")
            .map(|var| var.parse())
            .unwrap_or(Ok(DEFAULT_QUERY_INERVAL_SECS))?;

        let handle = tokio::spawn(async move {

            let mut interval = interval(Duration::from_secs(interval_secs));
            loop {
                let client = client.clone();
                match Self::query_metrics(client, &target_metrics).await {
                    Ok(metrics) => {
                        let json = serde_json::to_string_pretty(&metrics).unwrap();
                        log::debug!("{METRICS_ANNOT}:\n{json}");
                        sender.send(Message::AnnotationUpdate(vec![(METRICS_ANNOT, json)]))
                            .await
                            .expect("Failed to send message.");
                    },
                    Err(e) => log::error!("Failed to query Prometheus metrics: {e}")
                }
                interval.tick().await;
            }
        });
        
        self.task_handle = Some(handle);
        Ok(())
    }

    async fn query_metrics(client: Arc<Client>, target_metrics: &[Metric]) -> Result<HashMap<Arc<str>, f64>> {
        
        
        // Spawn a task to query each metric.
        let n_metrics = target_metrics.len();
        let mut joinset = JoinSet::new();
        for metric in target_metrics {
            let client = Arc::clone(&client);
            let name = Arc::clone(&metric.name);
            let query = Arc::clone(&metric.query);
            joinset.spawn(async move { 
                let result = client.query(query).get().await
                    .ok()
                    .and_then(|response| get_metric(response));
                
                (name, result)
            });
        }

        let mut metrics = HashMap::with_capacity(n_metrics);
        while let Some(res) = joinset.join_next().await {
            match res? {
                (metric, Some(value)) => {
                    log::info!("Succesfully prom. queried metric {metric}: {value}");
                    metrics.insert(metric, value); 
                },
                (metric, None) => log::error!("Failed to query metric: {metric}"),
            }
        }

        Ok(metrics)
    }
}

fn get_metric(result: PromqlResult) -> Option<f64> {

    let (_, sample) = result.into_inner().0
        .into_vector()
        .ok()
        .and_then(|vec| 
            vec.get(0).cloned()
        )
        .map(|metric| {
            metric.into_inner()
        })?;
    
    let value = sample.value();
    if value.is_nan() { Some(0.0) }
    else { Some(value) }
}
