use std::{collections::HashMap, env, str::FromStr, sync::Arc, time::Duration};
use prometheus_http_query::{response::PromqlResult, Client};
use tokio::{sync::mpsc, task::{JoinHandle, JoinSet}, time::interval};
use anyhow::Result;
use crate::{Message, METRICS_ANNOT};

const DEFAULT_QUERY_INERVAL_SECS: u64 = 60;

pub struct PrometheusClient {
    task_handle: Option<JoinHandle<()>>
}

impl Drop for PrometheusClient {
    fn drop(&mut self) {
        if let Some(handle) = &self.task_handle {
            handle.abort();
        }
    }
}

impl PrometheusClient {

    pub fn new(target: &str, sender: mpsc::Sender<Message<'static>>) -> Result<Self> {

        let mut client = Self {
            task_handle: None
        };

        client.run(target, sender)?;
        Ok(client)
    }

    fn run(&mut self, target: &str, sender: mpsc::Sender<Message<'static>>) -> Result<()> {
    
        let client = Arc::new(Client::from_str(target)?);
        let interval_secs: u64 = env::var("METRICS_QUERY_INTERVAL_SECS")
            .map(|var| var.parse())
            .unwrap_or(Ok(DEFAULT_QUERY_INERVAL_SECS))?;

        let handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));
            loop {
                let client = client.clone();
                match Self::query_metrics(client).await {
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

    async fn query_metrics(client: Arc<Client>) -> Result<HashMap<String, f64>> {
        
        // Query the names of all available metrics.
        let mut metric_names: Vec<(Option<String>, String)> = client.label_values("__name__")
            .get().await?
            .into_iter()
            .map(|metric| (None, metric))
            .collect();
        
        metric_names.push((
            Some("queue_avg_5m".to_string()),
            "avg(avg_over_time(nv_inference_queue_duration_us[5m]))".to_string()
        ));
            
        // Spawn a task to query each metric.
        let n_metrics = metric_names.len();
        let mut joinset = JoinSet::new();
        for metric in metric_names {
            let client = Arc::clone(&client);
            joinset.spawn(async move { 
                let result = client.query(&metric.1).get().await
                    .ok()
                    .and_then(|response| get_metric(response));
                
                (metric.0.unwrap_or(metric.1), result)
            });
        }

        let mut metrics = HashMap::with_capacity(n_metrics);
        while let Some(res) = joinset.join_next().await {
            match res? {
                (metric, Some(value)) => { metrics.insert(metric, value); },
                (metric, None) => log::error!("Failed to query metric: {metric}"),
            }
        }

        Ok(metrics)
    }
}

fn get_metric(result: PromqlResult) -> Option<f64> {

    let (_, sample) = result.into_inner().0
        .into_vector()
        .map(|mut vec| 
            vec.remove(0)
        )
        .map(|metric| {
            metric.into_inner()
        }).ok()?;

    Some(sample.value())
}
