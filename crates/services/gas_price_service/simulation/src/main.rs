use std::path::{
    Path,
    PathBuf,
};

use crate::service::{
    get_service_controller,
    ServiceController,
};
use clap::{
    Parser,
    Subcommand,
};
use csv::StringRecord;
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

pub mod data_sources;
pub mod service;

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct Record {
    l1_block_number: u64,
    l1_blob_fee_wei: u64,
    l2_block_number: u64,
    l2_gas_fullness: u64,
    l2_gas_capacity: u64,
    l2_byte_size: u64,
    l2_byte_capacity: u64,
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
    inner: Vec<(BlockInfo, Option<DaBlockCosts>)>,
}

impl Data {
    pub fn get_iter(&self) -> impl Iterator<Item = (BlockInfo, Option<DaBlockCosts>)> {
        self.inner.clone().into_iter()
    }
}

impl From<&Record> for BlockInfo {
    fn from(value: &Record) -> Self {
        // TODO: Also support `GenesisBlock` variant?
        return BlockInfo::Block {
            height: value.l2_block_number.try_into().unwrap(),
            gas_used: value.l2_gas_fullness,
            block_gas_capacity: value.l2_gas_capacity,
            block_bytes: value.l2_byte_size,
            block_fees: 0, // TODO
            gas_price: 0,  // TODO
        }
    }
}

struct SimulationResults {}

fn get_data(source: &DataSource) -> anyhow::Result<Data> {
    let records = match source {
        DataSource::File { path } => {
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
    let groups = records.iter().chunk_by(|record| record.l1_block_number);
    for (l1_block_number, block_records) in groups.into_iter() {
        let blocks: Vec<_> = block_records.into_iter().collect();
        let mut blocks_iter = blocks.iter().peekable();

        while let Some(block_record) = blocks_iter.next() {
            let l2_block: BlockInfo = (*block_record).into();
            let da_block_costs = blocks_iter.peek().is_none().then(|| {
                // TODO: Check if these are generated correctly.
                let bundle_id: u32 = l1_block_number.try_into().unwrap();
                let range = blocks.first().unwrap().l2_block_number as u32
                    ..=blocks.last().unwrap().l2_block_number as u32;
                let bundle_size_bytes: u32 =
                    blocks.iter().map(|r| r.l2_byte_size as u32).sum();
                let blob_cost_wei = block_record.l1_blob_fee_wei as u128;

                DaBlockCosts {
                    bundle_id,
                    l2_blocks: range,
                    bundle_size_bytes,
                    blob_cost_wei,
                }
            });
            data.push((l2_block, da_block_costs));
        }
    }

    let data = Data { inner: data };
    Ok(data)
}

async fn simulation(
    data: &Data,
    service_controller: &mut ServiceController,
) -> anyhow::Result<SimulationResults> {
    let res = SimulationResults {};
    for (block, maybe_costs) in data.get_iter() {
        service_controller.advance(block, maybe_costs).await?
        // GET GAS PRICE

        // MODIFY WITH GAS PRICE FACTOR

        // RECORD LATEST VALUES
    }
    Ok(res)
}

fn display_results(_results: SimulationResults) -> anyhow::Result<()> {
    // TODO
    Ok(())
}

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    data_source: DataSource,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let data = get_data(&args.data_source)?;
    let mut service_controller = get_service_controller().await;
    let results = simulation(&data, &mut service_controller).await?;
    display_results(results)?;
    Ok(())
}
