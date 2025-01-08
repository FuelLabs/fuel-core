use crate::service::{
    get_service_controller,
    ServiceController,
};
use fuel_core_gas_price_service::{
    common::utils::BlockInfo,
    v1::da_source_service::DaBlockCosts,
};

pub mod data_sources;
pub mod service;

pub struct Data {
    inner: Vec<(BlockInfo, Option<DaBlockCosts>)>,
}

impl Data {
    pub fn get_iter(&self) -> impl Iterator<Item = (BlockInfo, Option<DaBlockCosts>)> {
        self.inner.clone().into_iter()
    }
}

struct SimulationResults {}

fn get_data() -> anyhow::Result<Data> {
    // TODO
    let data = Data { inner: vec![] };
    Ok(data)
}
async fn simulation(
    data: &Data,
    service_controller: &mut ServiceController,
) -> anyhow::Result<SimulationResults> {
    let res = SimulationResults {};
    for (block, maybe_costs) in data.get_iter() {
        service_controller
            .advance_one_l2_block(block, maybe_costs)
            .await?
    }
    Ok(res)
}

fn display_results(_results: SimulationResults) -> anyhow::Result<()> {
    // TODO
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data = get_data()?;
    let mut service_controller = get_service_controller().await;
    let results = simulation(&data, &mut service_controller).await?;
    display_results(results)?;
    Ok(())
}
