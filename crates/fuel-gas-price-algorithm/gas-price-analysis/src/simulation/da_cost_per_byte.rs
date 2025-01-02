use crate::{
    fields_of_struct_in_order,
    prettify_number,
    utils::gen_noisy_signal,
    Source,
};
use std::iter;

use super::{
    block_data::{
        HasBlobFee,
        PredefinedRecord,
    },
    Predefined2Record,
};

const PREDEFINED_L2_BLOCKS_PER_L1_BLOCK: usize = 12;

pub fn get_da_cost_per_byte_from_source(
    source: &Source,
    update_period: usize,
) -> Vec<u64> {
    match source {
        Source::Generated { size } => arbitrary_cost_per_byte(*size, update_period)
            .into_iter()
            .flat_map(|x| iter::repeat(x).take(update_period))
            .collect(),
        Source::Predefined {
            file_path,
            sample_size,
        } => get_l1_costs_from_csv_file::<PredefinedRecord>(
            file_path,
            *sample_size,
            PREDEFINED_L2_BLOCKS_PER_L1_BLOCK,
        )
        .into_iter()
        .flat_map(|x| iter::repeat(x).take(update_period))
        .collect(),
        Source::Predefined2 {
            file_path,
            l2_blocks_per_blob,
        } => get_l1_costs_from_csv_file::<Predefined2Record>(
            file_path,
            None,
            *l2_blocks_per_blob,
        )
        .into_iter()
        .flat_map(|x| iter::repeat(x).take(*l2_blocks_per_blob))
        .collect(),
    }
}

// This function is used to read the CSV file and extract the blob fee from it.
// The `sample_size` and `l2_blocks_per_blob` parameters are used to tailor the
// behavior to the slightly different approaches used in the two different CSV files.
//
// "Predefined" CSV file format (aka legacy):
// 1. `sample_size` represents the number of L1 blocks we want to simulate.
// 2. we assume that there are 12 L2 blocks per L1 block.
// 3. because each row in the file is a different L1 block.
// 4. we need to pull `sample_size` / 12 records from the CSV file
// 5. for example, for sample size 1200, algorithm assumes 1200 L1 blocks and pulls 100 L1 blocks from the file
//
// "Predefined2" CSV file format:
// 1. `sample_size` is not used - we always pull all data from the input file
// 2. `l2_blocks_per_blob` is used to determine how many L2 blocks are in a single L1 blob (no assumptions)
// 3. each row in the file is a different L2 block, hence L1 entries are repeated
// 4. so we need to skip `l2_blocks_per_blob` records to get to the next L1 block
// 5. for example, for l2_blocks_per_blob = 12, algorithm takes 1 value, skips 11, takes another value, etc.
fn get_l1_costs_from_csv_file<T>(
    file_path: &str,
    sample_size: Option<usize>,
    l2_blocks_per_blob: usize,
) -> Vec<u64>
where
    T: serde::de::DeserializeOwned + HasBlobFee,
{
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(file_path)
        .unwrap();
    let headers = csv::StringRecord::from(fields_of_struct_in_order::<T>());

    let costs: Vec<_> = rdr
        .records()
        .step_by(l2_blocks_per_blob)
        .take(
            sample_size
                .map(|size| size / l2_blocks_per_blob)
                .unwrap_or(usize::MAX),
        )
        .map(|record| {
            let record: T = record.unwrap().deserialize(Some(&headers)).unwrap();
            record.blob_fee_wei()
        })
        .collect();

    println!(
        "Max cost: {}",
        prettify_number(costs.iter().max_by(|a, b| a.cmp(b)).unwrap())
    );
    costs
}
fn noisy_eth_price<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    const COMPONENTS: &[f64] = &[3.0, 4.0];
    let input = input.try_into().unwrap();
    gen_noisy_signal(input, COMPONENTS)
}

fn arbitrary_cost_per_byte(size: usize, update_period: usize) -> Vec<u64> {
    let actual_size = size.div_ceil(update_period);

    const ROUGH_COST_AVG: f64 = 5.0;

    (0u32..actual_size as u32)
        .map(noisy_eth_price)
        .map(|x| x * ROUGH_COST_AVG + ROUGH_COST_AVG) // Sine wave is between -1 and 1, scale and shift
        .map(|x| x as u64)
        .map(|x| std::cmp::max(x, 1))
        .flat_map(|x| iter::repeat(x).take(update_period))
        .take(size)
        .collect()
}
