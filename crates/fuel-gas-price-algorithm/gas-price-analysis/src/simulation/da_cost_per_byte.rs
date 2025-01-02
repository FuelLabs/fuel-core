use crate::{
    prettify_number,
    simulation::gen_noisy_signal,
    Source,
};
use serde_reflection::{
    Samples,
    Tracer,
    TracerConfig,
};
use std::iter;

const PREDEFINED_L2_BLOCKS_PER_L1_BLOCK: usize = 12;

pub fn get_da_cost_per_byte_from_source(
    source: Source,
    update_period: usize,
) -> Vec<u64> {
    match source {
        Source::Generated { size } => arbitrary_cost_per_byte(size, update_period),
        Source::Predefined {
            file_path,
            sample_size,
        } => get_costs_from_csv_file::<PredefinedRecord>(
            &file_path,
            sample_size,
            PREDEFINED_L2_BLOCKS_PER_L1_BLOCK,
        ),
        Source::Predefined2 {
            file_path,
            l2_blocks_per_blob,
        } => get_costs_from_csv_file::<Predefined2Record>(
            &file_path,
            None,
            l2_blocks_per_blob,
        ),
    }
    .into_iter()
    .flat_map(|x| iter::repeat(x).take(update_period))
    .collect()
}

trait HasBlobFee {
    fn blob_fee_wei(&self) -> u64;
}
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, Default, serde::Serialize)]
struct PredefinedRecord {
    block_number: u64,
    excess_blob_gas: u64,
    blob_gas_used: u64,
    blob_fee_wei: u64,
    blob_fee_wei_for_1_blob: u64,
    blob_fee_wei_for_2_blobs: u64,
    blob_fee_wei_for_3_blobs: u64,
}

impl HasBlobFee for PredefinedRecord {
    fn blob_fee_wei(&self) -> u64 {
        self.blob_fee_wei
    }
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct Predefined2Record {
    l1_block_number: u64,
    l1_blob_fee_wei: u64,
    l2_block_number: u64,
    l2_fulness: u64,
    l2_size: u64,
}

impl HasBlobFee for Predefined2Record {
    fn blob_fee_wei(&self) -> u64 {
        self.l1_blob_fee_wei
    }
}

fn fields_of_struct_in_order<T>() -> Vec<String>
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

fn get_costs_from_csv_file<T>(
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
        .step_by(l2_blocks_per_blob as usize)
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
