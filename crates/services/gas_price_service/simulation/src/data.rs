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
    collections::HashMap,
    fmt::Debug,
    ops::RangeInclusive,
    path::Path,
};

#[allow(dead_code)]
#[derive(Debug, Clone, serde::Deserialize)]
struct Record {
    l1_block_number: u64,
    l1_blob_fee_wei: u64,
    l1_blob_size_bytes: u64,
    blob_start_l2_block_number: u32,
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

#[derive(Debug, Clone)]
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

pub fn get_data<P: AsRef<Path> + Debug>(path: P) -> anyhow::Result<Data> {
    const DA_OFFSET: usize = 0; // Make configurable

    tracing::info!("Loading data from file: {:?}", path);
    let headers = csv::StringRecord::from(fields_of_struct_in_order::<Record>());
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)
        .unwrap();

    let records_res: Result<Vec<Record>, _> = rdr
        .records()
        .map(|line_entry| {
            let line = line_entry?;
            line.deserialize(Some(&headers))
        })
        .collect();
    let records = records_res?;

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
    let mut orphaned_l2_blocks = HashMap::new();
    for record in records.into_iter() {
        let l2_block: BlockInfo = (&record).into();

        if record.l1_block_number != 0 {
            tracing::debug!("adding new DA info: {:?}", record);
            // TODO: Check if these are generated correctly.
            let bundle_id = record.l1_block_number as u32; // Could be an arbitrary number, but we use L1 block number for convenience.
            let bundle_size_bytes = record.l1_blob_size_bytes as u32;
            let range = record.blob_start_l2_block_number..=record.l2_block_number as u32;
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

        data.push((l2_block, da_block_costs.clone()));
        orphaned_l2_blocks.insert(record.l2_block_number, l2_block);
        for costs in da_block_costs {
            for height in costs.l2_blocks.clone() {
                orphaned_l2_blocks.remove(&(height as u64));
            }
        }
    }
    let total_orphaned = orphaned_l2_blocks.len();
    let total_orphaned_bytes = orphaned_l2_blocks
        .values()
        .filter_map(|block| {
            if let BlockInfo::Block { block_bytes, .. } = block {
                Some(*block_bytes)
            } else {
                None
            }
        })
        .sum::<u64>();

    tracing::info!("Total orphaned L2 blocks: {}", total_orphaned);
    tracing::info!("Total orphaned L2 block bytes: {}", total_orphaned_bytes);
    let binding = orphaned_l2_blocks.clone();
    let sorted_orphans = binding.keys().sorted();
    tracing::info!(
        "First 10 orphaned blocks: {:?}",
        sorted_orphans.clone().take(10).collect::<Vec<_>>()
    );
    // separate orphaned blocks into ranges
    let mut orphaned_ranges = Vec::new();
    let mut current_range: Option<RangeInclusive<u64>> = None;
    for height in sorted_orphans {
        if let Some(range) = current_range.as_mut() {
            if (range.end() + 1) == *height {
                current_range = Some(*range.start()..=*height);
            } else {
                orphaned_ranges.push(range.clone());
                current_range = None;
            }
        } else {
            current_range = Some(*height..=*height);
        }
    }
    tracing::info!("Orphaned ranges: {:?}", orphaned_ranges);
    let lengths_of_ranges = orphaned_ranges
        .iter()
        .map(|range| range.end() - range.start() + 1)
        .collect::<Vec<_>>();
    tracing::info!("Lengths of ranges: {:?}", lengths_of_ranges);

    let data = Data { inner: data };
    Ok(data)
}
