use crate::{
    prettify_number,
    simulation::gen_noisy_signal,
    Source,
};
use std::iter;

pub fn get_da_cost_per_byte_from_source(
    source: Source,
    update_period: usize,
) -> Vec<u64> {
    match source {
        Source::Generated { size } => arbitrary_cost_per_byte(size, update_period),
        Source::Predefined {
            file_path,
            sample_size,
        } => {
            let original = get_costs_from_csv_file(&file_path, sample_size);
            original
                .into_iter()
                .map(|x| std::iter::repeat(x).take(update_period))
                .flatten()
                .collect()
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct Record {
    block_number: u64,
    excess_blob_gas: u64,
    blob_gas_used: u64,
    blob_fee_wei: u64,
    blob_fee_wei_for_1_blob: u64,
    blob_fee_wei_for_2_blobs: u64,
    blob_fee_wei_for_3_blobs: u64,
}

fn get_costs_from_csv_file(file_path: &str, sample_size: Option<usize>) -> Vec<u64> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(file_path)
        .unwrap();
    let mut costs = vec![];
    let headers = csv::StringRecord::from(vec![
        "block_number",
        "excess_blob_gas",
        "blob_gas_used",
        "blob_fee_wei",
        "blob_fee_wei_for_1_blob",
        "blob_fee_wei_for_2_blobs",
        "blob_fee_wei_for_3_blobs",
    ]);
    let mut max_cost = 0;
    for record in rdr.records().skip(1) {
        if let Some(size) = sample_size {
            if costs.len() >= size {
                break;
            }
        };
        let record: Record = record.unwrap().deserialize(Some(&headers)).unwrap();
        let cost = record.blob_fee_wei;
        if cost > max_cost {
            max_cost = cost;
        }
        costs.push(cost);
    }
    println!("Max cost: {}", prettify_number(max_cost));
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
        .map(|x| iter::repeat(x).take(update_period as usize))
        .flatten()
        .take(size as usize)
        .collect()
}
