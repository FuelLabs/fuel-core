use std::{
    collections::HashMap,
    fs::File,
    path::Path,
};

use fuel_core_types::fuel_types::BlockHeight;

use crate::types::{
    BlockCommitterCosts,
    Layer2BlockData,
};

#[derive(Debug, serde::Serialize)]
struct BlockSummary {
    l1_block_number: u64,
    l1_blob_fee_wei: u128,
    l2_block_number: u32,
    l2_gas_fullness: u64,
    l2_gas_capacity: u64,
    l2_byte_size: u64,
    l2_byte_capacity: u64,
}

fn summarise_data_for_block_committer_costs(
    costs: &BlockCommitterCosts,
    l2_data: &HashMap<BlockHeight, Layer2BlockData>,
) -> Vec<BlockSummary> {
    tracing::info!("Building summary for: {:?}", costs);
    let block_range_len: usize = costs.len().try_into().unwrap_or_default();
    let mut summaries = Vec::with_capacity(block_range_len);
    for block_height in costs.iter() {
        let Ok(block_height): Result<u32, _> = block_height.try_into() else {
            continue
        };
        let l2_data = l2_data.get(&block_height);
        if let Some(l2_data) = l2_data {
            summaries.push(BlockSummary {
                l1_block_number: *costs.da_block_height,
                l1_blob_fee_wei: *costs.cost,
                l2_block_number: block_height,
                l2_gas_fullness: *l2_data.gas_consumed,
                l2_gas_capacity: *l2_data.capacity,
                l2_byte_size: *l2_data.block_size,
                l2_byte_capacity: *l2_data.bytes_capacity,
            });
        }
    }
    summaries
}

pub fn summarise_available_data(
    output_file_path: &Path,
    costs: &[BlockCommitterCosts],
    l2_data: &HashMap<BlockHeight, Layer2BlockData>,
) -> Result<(), anyhow::Error> {
    let file_writer = File::create(output_file_path)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(file_writer);
    for block_costs_entry in costs {
        let summaries =
            summarise_data_for_block_committer_costs(block_costs_entry, l2_data);
        for summary in summaries {
            tracing::debug!("Serializing record: {:?}", summary);
            writer.serialize(summary)?;
        }
    }
    writer.flush()?;
    Ok(())
}
