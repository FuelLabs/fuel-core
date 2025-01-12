use std::{
    cmp::{
        max,
        min,
    },
    ops::RangeInclusive,
};

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
    tracing::debug!("Starting simulation");
    let mut results = SimulationResults::new();
    for (block, maybe_costs) in data.get_iter() {
        service_controller.advance(block, maybe_costs).await?;
        let gas_price = service_controller.gas_price();
        let (profit, cost, reward, cost_per_byte) =
            service_controller.profit_cost_reward_cpb()?;

        results.add_gas_price(gas_price);
        results.add_profit(profit);
        results.add_cost(cost);
        results.add_reward(reward);
        results.add_cost_per_byte(cost_per_byte);
    }
    tracing::debug!("Finished simulation");
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
    single_simulation(data, &mut service_controller).await
}

pub async fn optimization(data: Data) -> anyhow::Result<SimulationResults> {
    let start_gas_price = 25_000; // TODO: pass value
    let p = select_p_value(&data, start_gas_price).await?;
    let d = select_d_value(&data, p, start_gas_price).await?;
    tracing::info!("Optimal p: {}, optimal d: {}", p, d);
    let best_result = run_single_simulation(&data, p, d, start_gas_price).await?;
    Ok(best_result)
}

const ETH_IN_WEI: i128 = 1_000_000_000_000_000_000;

// do a minimization on the p component
// this will first choose a distribution of p values
// from there it will find the two p values with the best results
// it will then dissect between those two p values again to find a better p value
// "best" result is determined by choosing the result with the lowest average profit.
// Exit when the best profit is not improved or the number of tries is reached
//
// Each simulation can be run in a separate thread
async fn select_p_value(data: &Data, start_gas_price: u64) -> anyhow::Result<i64> {
    const DISSECT_SAMPLES: i128 = 100;
    let mut p_range = 1..=ETH_IN_WEI; // convert to i128 to be safe
    let mut best_p = 1;
    let mut best_profit = i128::MAX;
    let tries = 10;
    for _ in 0..tries {
        // split the p_range into DISSECT_SAMPLES
        let p_values = dissect_range(&p_range, DISSECT_SAMPLES)?;

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
                    let avg_abs_profit = avg_profit(&result)?;
                    Ok::<(i128, i64), anyhow::Error>((avg_abs_profit, p))
                })
            })
            .collect();

        const TICK_TIME_SECONDS: u64 = 5;
        const TIMEOUT_SECONDS: u64 = 60;
        let future_res = tick_until_future_returns_or_timeout(
            futures::future::join_all(futures),
            TICK_TIME_SECONDS,
            TIMEOUT_SECONDS,
        )
        .await?;
        let pairs_res: anyhow::Result<Vec<_>> = future_res
            .into_iter()
            .flat_map(|fut_res| fut_res.map_err(|err| anyhow::anyhow!(err)))
            .collect();

        // choose the one p value associated with the lowest average profit
        let mut pairs = pairs_res?;

        pairs.sort_by_key(|(profit, _)| *profit);
        tracing::debug!("pairs: {:?}", pairs);
        let ((new_best_profit, new_best_p), remaining) = pairs
            .split_first()
            .ok_or(anyhow::anyhow!("No best profit found"))?;
        tracing::debug!("remaining: {:?}", remaining);
        let ((_, second_best_p), _) = remaining
            .split_first()
            .ok_or(anyhow::anyhow!("No second best profit found"))?;

        tracing::info!("old best profit: {}, old best p: {}", best_profit, best_p);
        tracing::info!(
            "new best profit: {}, new best p: {}",
            new_best_profit,
            new_best_p
        );
        if *new_best_profit < best_profit {
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
    tracing::info!("best p: {}", best_p);
    Ok(best_p)
}

async fn tick_until_future_returns_or_timeout<F: std::future::Future + Unpin>(
    future: F,
    tick_time_seconds: u64,
    timeout_seconds: u64,
) -> anyhow::Result<F::Output> {
    let mut future = Box::pin(future);
    let mut timeout = Box::pin(tokio::time::sleep(tokio::time::Duration::from_secs(
        timeout_seconds,
    )));
    let mut time_elapsed = 0;
    loop {
        let tick_timeout = Box::pin(tokio::time::sleep(
            tokio::time::Duration::from_secs(tick_time_seconds),
        ));
        tokio::select! {
            res = &mut future => {
                return Ok(res);
            }
            _ = &mut timeout => {
                return Err(anyhow::anyhow!("Timeout: {} seconds", timeout_seconds));
            }
            _ = tick_timeout => {
                time_elapsed += tick_time_seconds;
                tracing::info!("Time elapsed: {}", time_elapsed);
            }
        }
    }
}

// do the same as with p for d, but use a predefined p value,
async fn select_d_value(
    data: &Data,
    p: i64,
    start_gas_price: u64,
) -> anyhow::Result<i64> {
    const DISSECT_SAMPLES: usize = 100;
    let mut d_range = 1..=ETH_IN_WEI;
    let mut best_d = 1;
    let mut best_profit = i128::MAX;
    let tries = 10;
    for _ in 0..tries {
        // split the d_range into DISSECT_SAMPLES
        let d_values = dissect_range(&d_range, DISSECT_SAMPLES as i128)?;

        tracing::info!("running for d_values: {:?}", d_values);

        // for each d value, run a single simulation in a separate task and return the results
        let futures: Vec<_> = d_values
            .into_iter()
            .map(|d| {
                let data = data.clone();
                tokio::spawn(async move {
                    let d: i64 = d.try_into()?;
                    let result =
                        run_single_simulation(&data, p, d, start_gas_price).await?;
                    let avg_abs_profit = avg_profit(&result)?;
                    Ok::<(i128, i64), anyhow::Error>((avg_abs_profit, d))
                })
            })
            .collect();

        // join all the futures and get the results
        // let pairs_res: anyhow::Result<Vec<_>> = futures::future::join_all(futures)
        //     .await
        //     .into_iter()
        //     .flat_map(|fut_res| fut_res.map_err(|err| anyhow::anyhow!(err)))
        //     .collect();

        const TICK_TIME_SECONDS: u64 = 5;
        const TIMEOUT_SECONDS: u64 = 60;
        let future_res = tick_until_future_returns_or_timeout(
            futures::future::join_all(futures),
            TICK_TIME_SECONDS,
            TIMEOUT_SECONDS,
        )
        .await?;
        let pairs_res: anyhow::Result<Vec<_>> = future_res
            .into_iter()
            .flat_map(|fut_res| fut_res.map_err(|err| anyhow::anyhow!(err)))
            .collect();

        // choose the one d value associated with the lowest average profit
        let mut pairs = pairs_res?;
        pairs.sort_by_key(|(profit, _)| *profit);
        let ((new_best_profit, new_best_d), remaining) = pairs
            .split_first()
            .ok_or(anyhow::anyhow!("No best profit found"))?;
        let ((_, second_best_d), _) = remaining
            .split_first()
            .ok_or(anyhow::anyhow!("No second best profit found"))?;

        tracing::info!("old best profit: {}, old best d: {}", best_profit, best_d);
        tracing::info!(
            "new best profit: {}, new best d: {}",
            new_best_profit,
            new_best_d
        );
        if *new_best_profit < best_profit {
            tracing::info!("second best d: {}", second_best_d);
            best_d = *new_best_d;
            best_profit = *new_best_profit;

            // update d range
            let start_d = min(best_d, *second_best_d);
            let end_d = max(best_d, *second_best_d);
            d_range = start_d.into()..=end_d.into();
        } else {
            break
        }
    }
    tracing::info!("best d: {}", best_d);
    Ok(best_d)
}

fn dissect_range(
    range: &RangeInclusive<i128>,
    samples: i128,
) -> anyhow::Result<Vec<i128>> {
    let mut values = Vec::new();
    let range_len = range
        .end()
        .checked_sub(*range.start())
        .ok_or(anyhow::anyhow!(
            "Overflow: {} - {}",
            range.end(),
            range.start()
        ))?;
    let interval_len = range_len.checked_div(samples).ok_or(anyhow::anyhow!(
        "Overflow: {} / {}",
        range_len,
        samples
    ))?;
    for i in 0..samples {
        let add = interval_len.checked_mul(i).ok_or(anyhow::anyhow!(
            "Overflow: {} * {}",
            interval_len,
            i
        ))?;
        let p = range.start().checked_add(add).ok_or(anyhow::anyhow!(
            "Overflow: {} + {}",
            range.start(),
            add
        ))?;
        values.push(p);
    }
    Ok(values)
}

// profit is the reward - known cost (not the predicted profit)
// use the abs of the profits so we large swings in profit don't cancel out
fn avg_profit(result: &SimulationResults) -> anyhow::Result<i128> {
    let profit_abs_sum = result
        .rewards
        .iter()
        .map(|r| *r as i128)
        .zip(result.costs.iter().map(|c| *c as i128))
        .map(|(r, c)| r.saturating_sub(c))
        .map(|p| p.abs())
        .sum::<i128>();
    let len = i128::try_from(result.profits.len())?;
    let avg_abs_profit = profit_abs_sum.checked_div(len).ok_or(anyhow::anyhow!(
        "Overflow: {} / {}",
        profit_abs_sum,
        len
    ))?;
    Ok(avg_abs_profit)
}
