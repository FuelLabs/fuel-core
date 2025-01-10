use crate::{
    data::Data,
    service::ServiceController,
};

#[derive(Debug)]
pub struct SimulationResults {
    pub gas_price: Vec<u64>,
    pub profits: Vec<i128>,
    pub costs: Vec<u128>,
    pub rewards: Vec<u128>,
    pub cost_per_byte: Vec<u128>,
}

impl SimulationResults {
    fn new() -> Self {
        Self {
            gas_price: Vec::new(),
            profits: Vec::new(),
            costs: Vec::new(),
            rewards: Vec::new(),
            cost_per_byte: Vec::new(),
        }
    }
}

impl SimulationResults {
    pub fn add_gas_price(&mut self, gas_price: u64) {
        self.gas_price.push(gas_price);
    }

    pub fn add_profit(&mut self, profit: i128) {
        self.profits.push(profit);
    }

    pub fn add_cost(&mut self, cost: u128) {
        self.costs.push(cost);
    }

    pub fn add_reward(&mut self, reward: u128) {
        self.rewards.push(reward);
    }

    pub fn add_cost_per_byte(&mut self, cost_per_byte: u128) {
        self.cost_per_byte.push(cost_per_byte);
    }
}

pub async fn simulation(
    data: &Data,
    service_controller: &mut ServiceController,
) -> anyhow::Result<SimulationResults> {
    tracing::info!("Starting simulation");
    let mut results = SimulationResults::new();
    let last_cost_per_byte = 0;
    for (block, maybe_costs) in data.get_iter() {
        let cost_per_byte = if let Some(costs) = maybe_costs.last() {
            costs.blob_cost_wei / costs.bundle_size_bytes as u128
        } else {
            last_cost_per_byte
        };
        service_controller.advance(block, maybe_costs).await?;
        let gas_price = service_controller.gas_price();
        let (profit, cost, reward) = service_controller.profit_cost_reward()?;

        results.add_gas_price(gas_price);
        results.add_profit(profit);
        results.add_cost(cost);
        results.add_reward(reward);
        results.add_cost_per_byte(cost_per_byte);
    }
    tracing::info!("Finished simulation");
    Ok(results)
}
