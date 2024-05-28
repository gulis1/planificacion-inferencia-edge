use std::path::Path;

use petgraph::{graphmap::DiGraphMap, visit::{EdgeRef, IntoEdges}, Direction};
use tokio::fs;
use tservice_lib::policy::{AsyncDefault, GraphWrapper, PodMap, Policy};
use uuid::Uuid;
use anyhow::{Context, Result};
use serde_json::Value as JsonValue;

const GRAPH_FILE_PATH: &str = "./graph.json";

#[derive(Debug)]
pub struct FromFile {
    target_graph: DiGraphMap<Uuid, ()>,
}

impl AsyncDefault for FromFile {
    
    async fn default() -> Self {
        let path = Path::new(GRAPH_FILE_PATH);
        if !path.exists() { log::error!("File ./graph.json not found."); }
        else { log::info!("Found graph.json file."); }

        let graph = parse_graph_file(path).await;
        match graph {
            Ok(g) => {
                log::info!("Succesfully parsed graph file");
                Self {
                    target_graph: g
                }
            },
            Err(e) => {
                log::error!("Failed to parse graph file: {e}");
                Self {
                    target_graph: DiGraphMap::new()
                }
            }
        }
    }
}

impl Policy for FromFile {

    fn pod_added(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: Uuid) -> Vec<Uuid> {
        
        if !self.target_graph.contains_node(pod) {
            log::warn!("The service does not container pod {pod}");
            return Vec::new();
        }
        
        let mut affected = Vec::new();

        // Añadir todos los nodos con conexienes entrantes.
        for (source, target, _) in self.target_graph.edges_directed(pod, Direction::Incoming) {
            if graph.contains_node(source) {
                graph.add_edge(source, target);
                affected.push(source);
            }
        }

        for (source, target, _) in self.target_graph.edges_directed(pod, Direction::Outgoing) {
            if graph.contains_node(target) {
                graph.add_edge(source, target);
                affected.push(target);
            }
        }

        affected
    }

    fn pod_removed(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: Uuid, affected: &[Uuid]) -> Vec<Uuid> {
        Vec::new()
    }

    fn pod_updated(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: Uuid) -> Vec<Uuid> {
        Vec::new()
    }
}

async fn parse_graph_file(file: &Path) -> Result<DiGraphMap<Uuid, ()>> {

    let content = fs::read_to_string(file).await?;
    let parsed: JsonValue = serde_json::from_str(&content)?;
    
    let dot_content = parsed.get("graph")
        .context("Json missing key 'graph'.")?
        .as_str().unwrap();

    let mut g: DiGraphMap<Uuid, ()> = DiGraphMap::new();
    let parser = rust_dot::parse_string(dot_content);
    
    for node in &parser.nodes {
        let uid: Uuid = node.parse()?;
        g.add_node(uid);
    }

    for edge in &parser.edges {
        let src: Uuid = parser.nodes.get(edge.0).unwrap().parse()?;
        let dst: Uuid = parser.nodes.get(edge.1).unwrap().parse()?;

        g.add_edge(src, dst, ());
    }

    Ok(g)
}
