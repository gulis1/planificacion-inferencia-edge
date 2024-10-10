use tservice_lib::policy::*;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct NoOp();

impl Policy for NoOp {
    fn pod_added(&mut self, _graph: &mut GraphWrapper, _pods_info: &PodMap, pod: Uuid) -> Vec<Uuid> {
        log::info!("NoOp for added pod: {pod}");
        Vec::new()
    }

    fn pod_removed(&mut self, _graph: &mut GraphWrapper, _pods_info: &PodMap, pod: Uuid, _affected: &[Uuid]) -> Vec<Uuid> {
        log::info!("NoOp for removed pod: {pod}");
        Vec::new()
    }

    fn pod_updated(&mut self, _graph: &mut GraphWrapper, _pods_info: &PodMap, pod: Uuid) -> Vec<Uuid> {
        log::info!("NoOp for updated pod: {pod}");
        Vec::new()
    }
}

impl AsyncDefault for NoOp {
    async fn default() -> Self {
        Self {} 
    }
}
