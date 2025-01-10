use crate::{
    cli::SimulationArgs,
    data::Data,
    service::{
        get_service_controller,
        ConfigValues,
        MetadataValues,
        ServiceController,
    },
};
use std::cmp::{
    max,
    min,
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

pub async fn single_simulation(
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

pub async fn run_a_simulation(
    args: SimulationArgs,
    data: Data,
) -> anyhow::Result<SimulationResults> {
    match args {
        SimulationArgs::Single {
            p_component,
            d_component,
            start_gas_price,
        } => {
            run_single_simulation(&data, p_component, d_component, start_gas_price).await
        }
        SimulationArgs::Optimization { .. } => optimization(data).await,
    }
}

async fn run_single_simulation(
    data: &Data,
    p_component: i64,
    d_component: i64,
    start_gas_price: u64,
) -> anyhow::Result<SimulationResults> {
    let starting_height = data.starting_height();
    let config_values = ConfigValues {
        min_da_gas_price: 1000,
        max_da_gas_price: u64::MAX,
        da_p_component: p_component,
        da_d_component: d_component,
    };
    let metadata_values = MetadataValues::new(starting_height, start_gas_price);
    let mut service_controller =
        get_service_controller(config_values, metadata_values).await?;
    single_simulation(&data, &mut service_controller).await
}

pub async fn optimization(data: Data) -> anyhow::Result<SimulationResults> {
    let start_gas_price = 1_000; // TODO: pass value
    let p = select_p_value(&data, start_gas_price).await?;
    let d = select_d_value(&data, p, start_gas_price).await?;
    let best_result = run_single_simulation(&data, p, d, start_gas_price).await?;
    Ok(best_result)
}

// do a minimization on the p component
// this will first choose a distribution of p values
// from there it will find the two p values with the best results
// it will then dissect between those two p values again to find a better p value
// "best" result is determined by choosing the result with the lowest average profit.
// Exit when the best profit is not improved or the number of tries is reached
//
// Each simulation can be run in a separate thread
async fn select_p_value(data: &Data, start_gas_price: u64) -> anyhow::Result<i64> {
    const DISSECT_SAMPLES: i128 = 10;
    let mut p_range = (i64::MIN as i128)..=(i64::MAX as i128); // convert to i128 to be safe
    let mut best_p = 1;
    let mut best_profit = i128::MAX;
    let tries = 10;
    for _ in 0..tries {
        // split the p_range into DISSECT_SAMPLES
        let mut p_values = Vec::new();
        let range_len =
            p_range
                .end()
                .checked_sub(*p_range.start())
                .ok_or(anyhow::anyhow!(
                    "Overflow: {} - {}",
                    p_range.end(),
                    p_range.start()
                ))?;
        let interval_len =
            range_len
                .checked_div(DISSECT_SAMPLES)
                .ok_or(anyhow::anyhow!(
                    "Overflow: {} / {}",
                    range_len,
                    DISSECT_SAMPLES
                ))?;
        for i in 0..DISSECT_SAMPLES {
            let add = interval_len.checked_mul(i).ok_or(anyhow::anyhow!(
                "Overflow: {} * {}",
                interval_len,
                i
            ))?;
            let p = p_range.start().checked_add(add).ok_or(anyhow::anyhow!(
                "Overflow: {} + {}",
                p_range.start(),
                add
            ))?;
            p_values.push(p);
        }

        tracing::info!("running for p_values: {:?}", p_values);

        // for each p value, run a single simulation in a separate task and return the results
        let futures: Vec<_> = p_values
            .into_iter()
            .map(|p| {
                let data = data.clone();
                tokio::spawn(async move {
                    let p: i64 = p.try_into()?;
                    let result =
                        run_single_simulation(&data, p, 0, start_gas_price).await?;
                    // use the abs of the profits so we large swings in profit don't cancel out
                    let profit_abs_sum =
                        result.profits.iter().map(|p| p.abs()).sum::<i128>();
                    let avg_abs_profit = profit_abs_sum / result.profits.len() as i128;
                    Ok::<(i128, i64), anyhow::Error>((avg_abs_profit, p))
                })
            })
            .collect();

        // join all the futures and get the results
        let mut pairs_res: anyhow::Result<Vec<_>> = futures::future::join_all(futures)
            .await
            .into_iter()
            .map(|fut_res| fut_res.map_err(|err| anyhow::anyhow!(err)))
            .flatten()
            .collect();

        // choose the one p value associated with the lowest average profit
        let mut pairs = pairs_res?;

        pairs.sort_by_key(|(profit, _)| *profit);
        let ((new_best_profit, new_best_p), remaining) = pairs
            .split_first()
            .ok_or(anyhow::anyhow!("No best profit found"))?;
        let ((_, second_best_p), _) = remaining
            .split_first()
            .ok_or(anyhow::anyhow!("No second best profit found"))?;

        if *new_best_profit < best_profit {
            tracing::info!(
                "new best profit: {}, new best p: {}",
                new_best_profit,
                new_best_p
            );
            tracing::info!("second best p: {}", second_best_p);
            best_p = *new_best_p;
            best_profit = *new_best_profit;

            // update p range
            let start_p = min(*second_best_p, *new_best_p);
            let end_p = max(*second_best_p, *new_best_p);
            p_range = start_p.into()..=end_p.into();
        } else {
            break
        }
    }

    Ok(best_p)
}

// do the same as with p for d, but use a predefined p value,
async fn select_d_value(
    data: &Data,
    p: i64,
    start_gas_price: u64,
) -> anyhow::Result<i64> {
    const DISSECT_SAMPLES: usize = 10;
    let mut d_range = i64::MIN..=i64::MAX;
    let mut best_d = 1;
    let mut best_profit = i128::MAX;
    let tries = 10;
    for _ in 0..tries {
        // split the d_range into DISSECT_SAMPLES
        let mut d_values = Vec::new();
        for i in 0..DISSECT_SAMPLES {
            let d = d_range.start()
                + (d_range.end() - d_range.start()) * i as i64 / DISSECT_SAMPLES as i64;
            d_values.push(d);
        }

        // for each d value, run a single simulation in a separate task and return the results
        let futures: Vec<_> = d_values
            .iter()
            .map(|d| {
                let data = data.clone();
                let d = *d;
                tokio::spawn(async move {
                    let result =
                        run_single_simulation(&data, p, d, start_gas_price).await?;
                    // use the abs of the profits so we large swings in profit don't cancel out
                    let profit_abs_sum =
                        result.profits.iter().map(|p| p.abs()).sum::<i128>();
                    let avg_abs_profit = profit_abs_sum / result.profits.len() as i128;
                    Ok::<(i128, i64), anyhow::Error>((avg_abs_profit, d))
                })
            })
            .collect();

        // join all the futures and get the results
        let mut pairs_res: anyhow::Result<Vec<_>> = futures::future::join_all(futures)
            .await
            .into_iter()
            .map(|fut_res| fut_res.map_err(|err| anyhow::anyhow!(err)))
            .flatten()
            .collect();

        // choose the one d value associated with the lowest average profit
        let pairs = pairs_res?;
        let (new_best_profit, new_best_d) = pairs
            .into_iter()
            .min_by_key(|(profit, _)| *profit)
            .ok_or(anyhow::anyhow!("No best profit found"))?;

        if new_best_profit < best_profit {
            best_d = new_best_d;
            best_profit = new_best_profit;
        } else {
            break
        }
    }
    Ok(best_d)
}
