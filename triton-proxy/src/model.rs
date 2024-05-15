use serde::Deserialize;
use anyhow::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    pub name: String,
    pub format: String,
    pub batch_size: u32,
    pub channels: u32,
    pub width: u32,
    pub height: u32,
    pub input_type: String,
    pub input_name: String,
    pub output_name: String,
    pub needs_gpu: bool,
    pub perf: u32,
    pub accuracy: u32
}

pub fn read_models(path: &str) -> Result<Vec<Model>> {
    
    let models: Vec<Model> = csv::Reader::from_path(path)?
        .deserialize()
        .flatten()
        .collect();

    log::info!("Found models: {:?}", models);
    Ok(models)
}
