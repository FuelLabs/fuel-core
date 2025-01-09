use fuel_core_types::fuel_types::BlockHeight;
use std::{
    collections::HashMap,
    fs::File,
    ops::Range,
    path::Path,
};

use crate::types::{
    BlockCommitterCosts,
    Layer2BlockData,
};

#[derive(Debug, serde::Serialize)]
struct BlockSummary {
    l1_block_number: u64,
    l1_blob_fee_wei: u128,
    l1_blob_size_bytes: u64,
    blob_start_l2_block_number: u32,
    l2_block_number: u32,
    l2_gas_fullness: u64,
    l2_gas_capacity: u64,
    l2_byte_size: u64,
    l2_byte_capacity: u64,
    l2_block_transactions_count: usize,
    l2_gas_price: u64,
    l2_fee: u128,
}

fn summarise_data_for_block_committer_costs(
    l2_height: u32,
    costs: &BlockCommitterCosts,
    l2_data: &HashMap<BlockHeight, Layer2BlockData>,
) -> BlockSummary {
    tracing::info!("Building summary for: {:?}", l2_height);
    let l2_data = l2_data.get(&l2_height).unwrap();
    let size = *costs.size;
    BlockSummary {
        l1_block_number: *costs.da_block_height,
        l1_blob_fee_wei: *costs.cost,
        l1_blob_size_bytes: size,
        blob_start_l2_block_number: *costs.start_height,
        l2_block_number: l2_height,
        l2_gas_fullness: *l2_data.gas_consumed,
        l2_gas_capacity: *l2_data.capacity,
        l2_byte_size: *l2_data.block_size,
        l2_byte_capacity: *l2_data.bytes_capacity,
        l2_block_transactions_count: l2_data.transactions_count,
        l2_gas_price: l2_data.gas_price,
        l2_fee: *l2_data.fee,
    }
}

pub fn summarise_available_data(
    l2_block_range: Range<BlockHeight>,
    output_file_path: &Path,
    costs: &HashMap<BlockHeight, BlockCommitterCosts>,
    l2_data: &HashMap<BlockHeight, Layer2BlockData>,
) -> Result<(), anyhow::Error> {
    let range: Range<u32> =
        l2_block_range.start.try_into()?..l2_block_range.end.try_into()?;
    let file_writer = File::create(output_file_path)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(file_writer);
    for l2_height in range {
        let costs = costs
            .get(&l2_height)
            .map(|costs| costs.clone())
            .unwrap_or(BlockCommitterCosts::empty());
        let summary =
            summarise_data_for_block_committer_costs(l2_height, &costs, l2_data);
        tracing::debug!("Serializing record: {:?}", summary);
        writer.serialize(summary)?;
    }
    writer.flush()?;
    Ok(())
}
