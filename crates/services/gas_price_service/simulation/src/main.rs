use crate::service::{
    get_service_controller,
    ServiceController,
};
use clap::{
    Parser,
    Subcommand,
};
use fuel_core_gas_price_service::{
    common::utils::BlockInfo,
    v1::da_source_service::DaBlockCosts,
};
use itertools::Itertools;
use serde_reflection::{
    Samples,
    Tracer,
    TracerConfig,
};
use std::{
    env,
    path::PathBuf,
};
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
    Layer,
};

pub mod data_sources;
pub mod service;

// l1_block_number,l1_blob_fee_wei,l1_blob_size_bytes,l2_block_number,l2_gas_fullness,l2_gas_capacity,l2_byte_size,l2_byte_capacity,l2_block_transactions_count,l2_gas_price,l2_fee
// 21403864,509018984154240,1436954,9099900,0,30000000,488,260096,1,1000,0
// 21403864,509018984154240,1436954,9099901,1073531,30000000,3943,260096,2,1000,934000000000
// 21403864,509018984154240,1436954,9099902,0,30000000,488,260096,1,1000,0
// 21403864,509018984154240,1436954,9099903,0,30000000,488,260096,1,1000,0
// 21403864,509018984154240,1436954,9099904,0,30000000,488,260096,1,1000,0
// 21403864,509018984154240,1436954,9099905,0,30000000,488,260096,1,1000,0
// 21403864,509018984154240,1436954,9099906,0,30000000,488,260096,1,1000,0
#[allow(dead_code)]
#[derive(Debug, Clone, serde::Deserialize)]
struct Record {
    l1_block_number: u64,
    l1_blob_fee_wei: u64,
    l1_blob_size_bytes: u64,
    l2_block_number: u64,
    l2_gas_fullness: u64,
    l2_gas_capacity: u64,
    l2_byte_size: u32,
    l2_byte_capacity: u64,
    l2_block_transactions_count: u64,
    l2_gas_price: u64,
    l2_fee: u64,
}

pub(crate) fn fields_of_struct_in_order<T>() -> Vec<String>
where
    T: serde::de::DeserializeOwned,
{
    let mut tracer = Tracer::new(TracerConfig::default());
    let samples = Samples::new();
    tracer.trace_type::<T>(&samples).unwrap();
    let type_name = std::any::type_name::<T>().split("::").last().unwrap();
    let registry = tracer.registry().unwrap();
    let Some(serde_reflection::ContainerFormat::Struct(fields)) = registry.get(type_name)
    else {
        panic!("No fields?")
    };

    fields.iter().map(|f| f.name.clone()).collect()
}

#[derive(Subcommand, Debug)]
enum DataSource {
    /// Load data from a CSV file.
    File {
        #[arg(short, long)]
        path: PathBuf,
    },
    /// Generate arbitrary data (not supported yet).
    Generated,
}

#[derive(Debug)]
pub struct Data {
    inner: Vec<(BlockInfo, Vec<DaBlockCosts>)>,
}

impl Data {
    pub fn get_iter(&self) -> impl Iterator<Item = (BlockInfo, Vec<DaBlockCosts>)> {
        self.inner.clone().into_iter()
    }

    pub fn starting_height(&self) -> u32 {
        self.inner
            .first()
            .and_then(|(block, _)| {
                if let BlockInfo::Block { height, .. } = block {
                    Some(*height)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }
}

impl From<&Record> for BlockInfo {
    fn from(value: &Record) -> Self {
        return BlockInfo::Block {
            height: value.l2_block_number.try_into().unwrap(),
            gas_used: value.l2_gas_fullness,
            block_gas_capacity: value.l2_gas_capacity,
            block_bytes: value.l2_byte_size as u64,
            block_fees: value.l2_fee, // Will be overwritten by the simulation code
            gas_price: value.l2_gas_price, // Will be overwritten by the simulation code
        }
    }
}

#[derive(Debug)]
struct SimulationResults {
    gas_price: Vec<u64>,
    profits: Vec<i128>,
}

impl SimulationResults {
    fn new() -> Self {
        Self {
            gas_price: Vec::new(),
            profits: Vec::new(),
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
}

fn get_data(source: &DataSource) -> anyhow::Result<Data> {
    const DA_OFFSET: usize = 0; // Make configurable

    let records = match source {
        DataSource::File { path } => {
            tracing::info!("Loading data from file: {:?}", path);
            let headers = csv::StringRecord::from(fields_of_struct_in_order::<Record>());
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_path(path)
                .unwrap();

            let records: Result<Vec<Record>, _> = rdr
                .records()
                .map(|line_entry| {
                    let line = line_entry?;
                    line.deserialize(Some(&headers))
                })
                .collect();
            records?
        }
        DataSource::Generated => unimplemented!(),
    };

    let mut data = vec![];
    // let groups = records.iter().chunk_by(|record| record.l1_block_number);

    let mut latest_da_costs = Vec::new();

    tracing::info!("record length: {:?}", records.len());
    let l1_blocks: Vec<_> = records
        .iter()
        .map(|record| record.l1_block_number)
        .unique()
        .collect();
    tracing::info!("l1_blocks: {:?}", l1_blocks);
    for record in records.into_iter() {
        // let blocks: Vec<_> = block_records.into_iter().collect();
        // let mut blocks_iter = blocks.iter().peekable();

        // while let Some(block_record) = blocks_iter.next() {
        let l2_block: BlockInfo = (&record).into();

        if record.l1_block_number != 0 {
            tracing::info!("adding new DA info: {:?}", record);
            // TODO: Check if these are generated correctly.
            let bundle_id = record.l1_block_number as u32; // Could be an arbitrary number, but we use L1 block number for convenience.
            let bundle_size_bytes = record.l1_blob_size_bytes as u32;
            let range = record.l2_block_number as u32..=record.l2_block_number as u32;
            let blob_cost_wei = record.l1_blob_fee_wei as u128;

            let new = DaBlockCosts {
                bundle_id,
                l2_blocks: range,
                bundle_size_bytes,
                blob_cost_wei,
            };
            latest_da_costs.push((new, DA_OFFSET + 1));
        }

        let mut da_block_costs = Vec::new();
        latest_da_costs = latest_da_costs
            .into_iter()
            .filter_map(|(cost, timer)| {
                if timer == 0 {
                    da_block_costs.push(cost);
                    None
                } else {
                    Some((cost, timer - 1))
                }
            })
            .collect();

        data.push((l2_block, da_block_costs));
        // }
    }

    let data = Data { inner: data };
    Ok(data)
}

async fn simulation(
    data: &Data,
    service_controller: &mut ServiceController,
) -> anyhow::Result<SimulationResults> {
    tracing::info!("Starting simulation");
    let mut results = SimulationResults::new();
    for (block, maybe_costs) in data.get_iter() {
        service_controller.advance(block, maybe_costs).await?;
        let gas_price = service_controller.gas_price();
        results.add_gas_price(gas_price);
        let profit = service_controller.profit()?;
        results.add_profit(profit);
    }
    tracing::info!("Finished simulation");
    Ok(results)
}

fn display_results(results: SimulationResults) -> anyhow::Result<()> {
    // TODO: Just have basic stuff now
    const WEI_PER_ETH: f64 = 1_000_000_000_000_000_000.0;
    let profits_eth = results
        .profits
        .iter()
        .map(|profit| *profit as f64 / WEI_PER_ETH)
        .collect::<Vec<_>>();
    // println!("Gas prices (Wei): {:?}", results.gas_price);
    // println!("Profits (ETH): {:?}", profits_eth);
    let max_gas_price = results.gas_price.iter().max();
    let max_profit = profits_eth.iter().max_by(|a, b| a.partial_cmp(b).unwrap());
    println!("Max gas price: {:?}", max_gas_price);
    println!("Max profit ETH: {:?}", max_profit);
    Ok(())
}

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    data_source: DataSource,
}

fn configure_tracing() {
    let filter = match env::var_os("RUST_LOG") {
        Some(_) => {
            EnvFilter::try_from_default_env().expect("Invalid `RUST_LOG` provided")
        }
        None => EnvFilter::new("info"),
    };

    let fmt = tracing_subscriber::fmt::Layer::default()
        .with_level(true)
        .boxed();

    tracing_subscriber::registry().with(fmt).with(filter).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();

    let args = Args::parse();
    let data = get_data(&args.data_source)?;
    let starting_height = data.starting_height();
    let mut service_controller = get_service_controller(starting_height).await?;
    let results = simulation(&data, &mut service_controller).await?;
    display_results(results)?;
    Ok(())
}
