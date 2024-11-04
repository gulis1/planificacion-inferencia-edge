#![allow(unused)]
mod random;
mod min_latencia;
mod min_queue;
mod requisitos;
mod rrobin;


pub use random::Random;
pub use rrobin::Rrobin;
pub use min_latencia::MinLatencia;
pub use requisitos::Requisitos;

use std::time::Instant;

use anyhow::{Result, anyhow};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::TcpStream
};
use edge_proxy_lib::{
    hardware::{get_hardware_info, SystemInfo},
    policy::{Receiver, Request, RequestContext, Sender},
    server::{Endpoint, Endpoints}
};

#[derive(Debug, Clone)]
pub struct SimpleContext {
    pub priority: u32,
    pub accuracy: u32,
    pub model: Option<String>
}

type TritonRequest = Request<SimpleContext>;
type TritonEndpoint = Endpoint<SimpleContext>;
type TritonEndpoints = Endpoints<SimpleContext>;

impl RequestContext for SimpleContext {
    async fn receive(reader: &mut Receiver<'_>) -> Result<Self, std::io::Error> {
        
        let priority = reader.r.read_u32().await?;
        let accuracy = reader.r.read_u32().await?;
        let model_length = reader.r.read_u32().await?;
        let model = if model_length == 0 { None }
            else { 
                let mut buff: Vec<u8> = vec![0_u8; model_length as usize];
                reader.r.read_exact(&mut buff);
                Some(String::from_utf8(buff).unwrap())
            };

        Ok(Self {
            priority,
            accuracy,
            model
        })
    }

    async fn send(sender: &mut Sender<'_>, req: &Self) -> Result<(), std::io::Error> {
        sender.s.write_u32(req.priority).await?;
        sender.s.write_u32(req.accuracy).await?;
        match req.model {
            Some(ref model) => {
                sender.s.write_u32(model.len() as u32).await?;
                sender.s.write_all(model.as_bytes()).await?;
            },
            None => sender.s.write_u32(0).await?
        }

        Ok(())
    }
}

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
    pub compatible_gpus: String,
    pub perf: u32,
    pub accuracy: u32
}

pub fn read_models(path: &str) -> Result<Vec<Model>> {
    
    let mut hw_info = get_hardware_info();

    // Ñapa para diferenciar la orin normal de la nano.
    if hw_info.gpus.get(0).is_some_and(|gpu| gpu.name == "Orin" && gpu.core_count == 1024) {
        hw_info.gpus[0].name.push_str("_nano");
    }


    let mut models: Vec<Model> = csv::Reader::from_path(path)?
        .deserialize()
        .flatten()
        .collect();

    log::info!("Found models: {:?}", models);
    let available_gpus: Vec<&str> = hw_info.gpus.iter().map(|g| g.name.as_str()).collect();
    log::info!("Available gpus: {:?}", available_gpus);
    models = models.into_iter()
        .filter(|m| {
            if m.compatible_gpus.len() == 0 {
                // Modelo solo para CPU, compatible con todo
                log::info!("Found compatible model: {}", m.name);
                true
            }
            else {
                m.compatible_gpus.split(";")
                    .any(|model_gpu| {
                        let matches = available_gpus.contains(&model_gpu);
                        if matches { log::info!("Found compatible model: {}", m.name); }
                        else { log::info!("Found incompatible model: {}", m.name); }
                        matches
                    })
            }
        })
        .collect();

    if models.len() == 0 {
        return Err(anyhow!("Found 0 compatible models."));
    }

    log::info!("Compatible models: {:?}", models); 
    Ok(models)
}

async fn process_locally(request: &Request<impl RequestContext>, model: &Model) -> Result<Vec<u8>> {
    
    let i1 = Instant::now();
    log::info!("Locally processing request: {}", request.id);
    let sock = TcpStream::connect("127.0.0.1:12345").await?;
    let (reader, writer) = sock.into_split();
    let mut reader = BufReader::new(reader); 
    let mut writer = BufWriter::new(writer);

    writer.write_u32(model.name.len() as u32).await?;
    writer.write_all(model.name.as_bytes()).await?;
    writer.write_all(&request.content).await?;
    writer.flush().await?;
    drop(writer);

    let mut output_buffer = Vec::with_capacity(1024);
    reader.read_to_end(&mut output_buffer).await?;
    log::info!("Tiempo de inferencia: {}ms", i1.elapsed().as_millis());
    output_buffer.extend_from_slice("Model: ".as_bytes());
    output_buffer.extend_from_slice(model.name.as_bytes());
    Ok(output_buffer)
}
