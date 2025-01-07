use std::collections::HashMap;

// const PATH: &str = "data/gas_costs_data.csv";
const PATH: &str = "/Users/jamesturner/fuel/fuel-core/crates/fuel-gas-price-algorithm/gas-price-data-fetcher/data/scrape_2.csv";
const WEI_PER_ETH: f64 = 1_000_000_000_000_000_000.;
// l1_block_number,l1_blob_fee_wei,l2_block_number,l2_gas_fullness,l2_gas_capacity,l2_byte_size,l2_byte_capacity
// 21403864,509018984154240,9099900,0,30000000,488,260096
// 21403864,509018984154240,9099901,1073531,30000000,3943,260096
// 21403864,509018984154240,9099902,0,30000000,488,260096
// parse data
#[derive(Debug, serde::Deserialize, Eq, PartialEq, Hash)]
struct Record {
    l1_block_number: u64,
    l1_blob_fee_wei: u128,
    l2_block_number: u64,
    l2_gas_fullness: u64,
    l2_gas_capacity: u64,
    l2_byte_size: u64,
    l2_byte_capacity: u64,
}
fn get_records_from_csv_file(file_path: &str) -> Vec<Record> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(file_path)
        .unwrap();
    let headers = csv::StringRecord::from(vec![
        "l1_block_number",
        "l1_blob_fee_wei",
        "l2_block_number",
        "l2_gas_fullness",
        "l2_gas_capacity",
        "l2_byte_size",
        "l2_byte_capacity",
    ]);
    let records = rdr
        .records()
        .skip(1)
        .map(|r| r.unwrap().deserialize(Some(&headers)).unwrap())
        .collect::<Vec<Record>>();
    records
}

fn main() {
    let records = get_records_from_csv_file(PATH);
    let length = records.len();
    let costs = records
        .iter()
        .map(|r| (r.l1_block_number, r.l1_blob_fee_wei))
        .collect::<HashMap<_, _>>();
    let total_costs: u128 = costs.values().sum();
    let total_l2_gas = records.iter().map(|r| r.l2_gas_fullness).sum::<u64>();

    // println!("Average cost: {}", average);
    println!("Length: {}", length);
    println!("Total cost: {}", total_costs);
    println!("Total cost (ETH): {}", total_costs as f64 / WEI_PER_ETH);
    println!(
        "Average cost per l2 block: {}",
        total_costs / length as u128
    );
    println!(
        "Average cost per l2 block (ETH): {}",
        (total_costs as f64 / length as f64) / WEI_PER_ETH
    );
    // get cost per l2 gas fullness
    let average_cost_per_l2_gas_fullness = total_costs / total_l2_gas as u128;
    println!(
        "Average cost per l2 gas: {}",
        average_cost_per_l2_gas_fullness
    );
}
