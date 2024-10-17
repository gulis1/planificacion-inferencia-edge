use log::info;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use uuid::Uuid;use serde_json::Value as JsonValue;
use anyhow::{Context, Result};
use std::{
    collections::BTreeMap, env, net::SocketAddr, str::FromStr, sync::Arc, time::{Duration, Instant}
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{TcpListener, TcpSocket, TcpStream},
    process::Command,
    sync::{oneshot, RwLock, Semaphore},
    time::{sleep, timeout}
};

use crate::{
    policy::{Policy, Receiver, Request, RequestContext, Sender},
    Message, MsgSender, HW_ANNOT, METRICS_ANNOT
};

/// Tiempo que tiene que pasar hasta que se vuelvan
/// a consultar las metricas de un vecino.
const QUERY_MAX_ELLAPSED: Duration = Duration::from_secs(10);

/// Numero máximo de consultas de métricas de vecinos
/// simultáneas.
const MAX_CONCURRENT_METRICS_QUERY: usize = 2;


pub type Endpoints<R> = BTreeMap<Uuid, Endpoint<R>>;

#[derive(Debug, Clone)]
pub struct PreviousResult<R: RequestContext> {
    pub duration: Option<Duration>,
    pub instant: Instant,
    pub context: R
}

#[derive(Debug)]
pub struct Endpoint<R: RequestContext> {
    pub name: Arc<str>,
    pub ip: Arc<str>,
    pub hw_info: Option<JsonValue>,
    pub metrics: Option<JsonValue>,
    pub metrics_queried_at: Option<Instant>,
    
    /// Infomation about the last query to this endpoint:
    /// - None if the endpoint has never been used.
    /// - Ok(Duration) if the last request as succesfull and took Duration
    /// - Err(Instant) if the last request didnt complete. Stores the instant the request was made.
    pub last_results: AllocRingBuffer <PreviousResult<R>>

}

pub(crate) struct ProxyServer<T, R> 
where T: Policy<R>,
      R: RequestContext
{
    self_uuid: Uuid,
    endpoints: RwLock<Endpoints<R>>,
    query_sem: Semaphore,
    sender: MsgSender<'static>,
    policy: T,
    request_timeout: Duration
}

impl<T, R> ProxyServer<T, R> 
where T: Policy<R> + 'static, 
      R: RequestContext + 'static
{
    pub async fn new(
        self_uuid: Uuid,
        listen_address: &str,
        sender: MsgSender<'static>,
        policy: T
    ) -> Result<Arc<Self>> {
    
        let timeout_ms: u64 = env::var("EDGE_PROXY_REQUEST_TIMEOUT_MS")
            .ok()
            .and_then(|x| x.parse().ok())
            .unwrap_or(5000u64);

        log::info!("Timeout found: {timeout_ms} ms");

        let server = Arc::new(Self {
            self_uuid,
            endpoints: RwLock::new(Endpoints::default()),
            query_sem: Semaphore::new(MAX_CONCURRENT_METRICS_QUERY),
            sender,
            policy,
            request_timeout: Duration::from_millis(timeout_ms)
        });
        
        server.run(listen_address).await?;
        Ok(server)
    }

    pub fn update_endpoints(self: &Arc<Self>, new_endpoints: JsonValue) {
    
        let server = Arc::clone(self);
        tokio::spawn(async move {
            let parsed = parse_endpoints(new_endpoints);
            let mut write_handle = server.endpoints.write().await;
            match parsed {
                Ok(mut endps) => {
                    for (uuid, ep) in endps.iter_mut() {
                        // Save metrics for previous endpoints that are
                        // still valid.
                        if let Some(old_endp) = write_handle.remove(uuid) {
                            ep.hw_info = old_endp.hw_info;
                            ep.metrics = old_endp.metrics;
                            ep.metrics_queried_at = old_endp.metrics_queried_at;
                            ep.last_results = old_endp.last_results;
                        }
                        else {
                            server.query_annot(*uuid, HW_ANNOT, &ep.name);
                        }
                    }
                    *write_handle = endps;
                }
                Err(e) => log::error!("Failed to parse new endpoints. {e}")
            };
        });
    }

    async fn run(self: &Arc<Self>, listen_address: &str) -> Result<()> {
        
        let listener = TcpListener::bind(listen_address).await?;
        let server = Arc::clone(self);
        // Tarea para el servidor proxy.
        tokio::spawn(async move {
            loop {
                let client = listener.accept().await;
                if let Ok((conn, addr)) = client {

                    let server = Arc::clone(&server);
                    tokio::spawn(async move {
                        let before = Instant::now();
                        let res = server.handle_request(conn, addr).await;
                        match res {
                            Ok(_) => log::info!("Petition processed in {} ms", before.elapsed().as_millis()),
                            Err(e) => log::error!("Proxy connection failed: {e}")
                        }
                    });
                }
            }
        });
        
        let server = Arc::clone(self);
        // Tarea para ir pidiendo metricas actualizadasde los vecinos.
        tokio::spawn(async move {
             
            loop {
                let endpoints = server.endpoints.read().await;
                for (uuid, ep) in endpoints.iter() {
                    // Si no hay que seguir esperando...
                    if !ep.metrics_queried_at.is_some_and(|t| t.elapsed() < QUERY_MAX_ELLAPSED) {
                        server.query_annot(*uuid, METRICS_ANNOT, &ep.name);
                    }
                }

                drop(endpoints);
                sleep(QUERY_MAX_ELLAPSED).await;
            }
        });
        

        Ok(())
    }

    async fn handle_request(self: Arc<Self>, mut client_conn: TcpStream, _from_addr: SocketAddr)
        -> Result<()>
    {
        let mut request = read_request(&mut client_conn).await?;
        log::info!("Received request: {:?}", request);

        let read_handle = self.endpoints.read().await;
        let target_uuid = self.policy.choose_target(&mut request, &read_handle).await;

        let target = read_handle.get(&target_uuid).context("Endpoint dropped")?;
        
        //if target.metrics_queried_at.is_some_and(|queried_at| queried_at.elapsed() > QUERY_MAX_ELLAPSED) {
        //    self.query_annot(target_uuid, METRICS_ANNOT, &target.name);
        //}

        let start = Instant::now();
        let result = if target_uuid != self.self_uuid {
            log::info!("EDGE_PROXY_DEBUG {} {} {}", request.id, request.jumps, target_uuid);
            let addr = SocketAddr::from_str(&target.ip)?;
            drop(read_handle);

            request.jumps += 1;
            request.previous_nodes.push(self.self_uuid);
            timeout(self.request_timeout, proxy(&mut client_conn, addr, &request))
                .await
                .ok()
        }
        else {
            drop(read_handle);

            let response = self.policy.process_locally(&request).await?;
            let mut writer = BufWriter::new(client_conn);
            writer.write_all(&response).await?;
            writer.write_all("\nRoute: ".as_bytes()).await?;
            for node in request.previous_nodes {
                writer.write_all(node.to_string().as_bytes()).await?;
                writer.write_all("->".as_bytes()).await?;
            }
            writer.write_all(self.self_uuid.to_string().as_bytes()).await?;
            writer.write_all("\n".as_bytes()).await?;
            writer.flush().await?;
            Some(Ok(()))
        };
        // On sucess, store how long it took for the target to answer the last request.
        // On timeout or error, store the instant the error.
        let now = Instant::now();
        let event = match &result {
            Some(Ok(_)) => PreviousResult { duration: Some(start.elapsed()), context: request.context, instant: now },
            _ => PreviousResult { duration: None, instant: now, context: request.context }
        };
        let mut write_handle = self.endpoints.write().await;
        if let Some(ep) = write_handle.get_mut(&target_uuid) {
            ep.last_results.push(event);
        }
        drop(write_handle);
        result.with_context(|| format!("Timeout expired for request {}", request.id))??;
        info!("Conexión terminada.");
        Ok(())
    }

    fn query_annot(self: &Arc<Self>, pod_uuid: Uuid, annot_name: &str, pod_name: &Arc<str>) {
        
        let server = Arc::clone(self);
        let pod_name = Arc::clone(pod_name);
        let annot_name = annot_name.to_string();
        tokio::spawn(async move {
            
            let permit = server.query_sem.acquire().await.unwrap();
            
            let (s, r) = oneshot::channel();
            let message = Message::NeighborAnnotRequest { 
                neighbor_name: Arc::clone(&pod_name),
                annot_name: annot_name.clone(),
                respond_to: s
            };
            server.sender.send(message).await.expect("Failed to send message.");
            let response = r.await.expect("Failed to get answer.");
            
            let mut endp_write = server.endpoints.write().await;
            if let Some(endp) = endp_write.get_mut(&pod_uuid) {

                // TODO: cambiar esta ñapa de ifs.
                if annot_name == METRICS_ANNOT {
                    endp.metrics_queried_at = Some(Instant::now());
                    endp.metrics = response;
                    if endp.metrics.is_none() {
                        log::warn!("Failed to query {annot_name} for pod {pod_name}")
                    }
                }
                else if annot_name == HW_ANNOT {
                    println!("Received hw_info por pod {pod_uuid}");
                    endp.hw_info = response;
                }
                            }
            drop(endp_write);
            drop(permit);
        });
    }
}

async fn proxy(client: &mut TcpStream, addr: SocketAddr, request: &Request<impl RequestContext>) -> Result<()> {
    
    let mut target = TcpSocket::new_v4()?
        .connect(addr)
        .await?;
    
    send_request(&mut target, request).await?;

    let mut response = Vec::new();
    target.read_to_end(&mut response).await?;
    client.write_all(&response).await?;
    Ok(())
}

async fn read_request<R: RequestContext> (stream: &mut TcpStream) -> Result<Request<R>> {

    let mut reader = Receiver {r: BufReader::new(stream) };
    let mut uuid_buff = [0_u8; 16];

    reader.r.read_exact(&mut uuid_buff).await?;
    let uuid = Uuid::from_slice(&uuid_buff)?;
    let jumps = reader.r.read_u32().await?;
    log::info!("Recibido: JUMPS {}", jumps);

    // Lectura del context
    let context = R::receive(&mut reader).await?;
    // let priority = reader.read_u8().await?;
    // let accuracy = reader.read_u8().await?;

    let request_size = reader.r.read_u64().await?;
    let mut content = vec![0; request_size as usize];
    reader.r.read_exact(content.as_mut_slice()).await?;
    
    let mut previous_nodes = Vec::with_capacity(jumps as usize);
    for _ in 0..jumps {
        reader.r.read_exact(&mut uuid_buff).await?;
        let uuid = Uuid::from_slice(&uuid_buff)?;
        log::info!("Recibido: UUID {}", uuid);
        previous_nodes.push(uuid)
    }

    Ok(Request {
        id: uuid,
        jumps,
        context,
        content,
        previous_nodes
    })
}

async fn send_request<R: RequestContext>(stream: &mut TcpStream, request: &Request<R>) -> Result<()> {

    let mut writer = Sender{ s: BufWriter::new(stream) };
    writer.s.write_all(request.id.as_bytes()).await?;
    writer.s.write_u32(request.jumps).await?;
    log::info!("Enviado: JUMPS {}", request.jumps);

    // Enviar el context
    R::send(&mut writer, &request.context).await?;
    //writer.write_u8(request.priority).await?;
    //writer.write_u8(request.accuracy).await?;
    writer.s.write_u64(request.content.len() as u64).await?;
    writer.s.write_all(&request.content).await?;
    for uuid in request.previous_nodes.iter() {
        writer.s.write_all(uuid.as_bytes()).await?;
        log::info!("Enviado: UUID {}", uuid);
    }

    writer.s.flush().await?;
    Ok(())
}

fn parse_endpoints<R: RequestContext>(mut json: JsonValue) -> Result<Endpoints<R>> {
    
    json.as_array_mut()
        .context("JSON is not an array.")?
        .into_iter()
        .map(|item| {
            let uuid = Uuid::from_str(
                item.get_mut("uuid")
                    .context("Endpoint missing uuid field.")?
                    .as_str().context("Invalid uuid.")?
            )?;
            let name = Arc::from(item.get_mut("name")
                .context("Endpoint missing name field.")?
                .take()
                .as_str().context("Invalid name,")?
            );

            let ip_field = item.get_mut("ip")
                .context("Endpoint missing ip field.")?
                .take();
                
            let ip = ip_field.as_str().context("Invalid IP.")?;
            Ok((uuid, Endpoint { name,
                ip: Arc::from(format!("{ip}:9999")), // TODO: puerto configurable. 
                hw_info: None,
                metrics: None,
                metrics_queried_at: None,
                last_results: AllocRingBuffer::new(5)
            }))
        })
        .collect()
}
