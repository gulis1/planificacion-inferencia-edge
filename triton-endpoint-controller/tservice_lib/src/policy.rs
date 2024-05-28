use std::collections::BTreeMap;
use k8s_openapi::api::core::v1::Pod;
use petgraph::{
    Directed, Direction,
    graphmap::{DiGraphMap, EdgesDirected, Nodes},
};
use uuid::Uuid;

pub type PodMap = BTreeMap<Uuid, Pod>;
pub type PodGraph = DiGraphMap<Uuid, ()>;
pub struct GraphWrapper<'a> {
    _graph: &'a mut PodGraph
}
impl<'a> GraphWrapper<'a> {

    pub fn new(graph: &'a mut PodGraph) -> Self {
        Self {
            _graph: graph
        }
    }

    pub fn node_count(&self) -> usize {
        self._graph.node_count()
    }

    pub fn contains_node(&self, node: Uuid) -> bool {
        self._graph.contains_node(node)
    }

    pub fn nodes(&self) -> Nodes<'_, Uuid> {
        self._graph.nodes()
    }

    pub fn edges_directed(&self, node: Uuid, dir: Direction) -> EdgesDirected<'_, Uuid, (), Directed> {
        self._graph.edges_directed(node, dir)
    }

    pub fn contains_edge(&self, a: Uuid, b: Uuid) -> bool {
        self._graph.contains_edge(a, b)
    }

    pub fn add_edge(&mut self, from: Uuid, to: Uuid) {
        self._graph.add_edge(from, to, ());
    }

    pub fn remove_edge(&mut self, from: Uuid, to: Uuid) -> Option<()> {
        self._graph.remove_edge(from, to)
    }
}

pub trait AsyncDefault: Send {
    fn default() -> impl std::future::Future<Output = Self> + std::marker::Send;
}

pub trait Policy: AsyncDefault + std::fmt::Debug + Send {
    fn pod_added(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: Uuid) -> Vec<Uuid>;
    fn pod_removed(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: Uuid, affected: &[Uuid]) -> Vec<Uuid>;
    fn pod_updated(&mut self, graph: &mut GraphWrapper, pods: &PodMap, pod: Uuid) -> Vec<Uuid>;
}



