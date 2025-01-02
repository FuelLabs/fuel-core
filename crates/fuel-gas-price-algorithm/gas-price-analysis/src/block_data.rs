use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use crate::utils::gen_noisy_signal;

pub(super) trait HasBlobFee {
    fn blob_fee_wei(&self) -> u64;
}
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, Default, serde::Serialize)]
pub(super) struct PredefinedRecord {
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
pub(super) struct Predefined2Record {
    l1_block_number: u64,
    l1_blob_fee_wei: u64,
    l2_block_number: u64,
    l2_fullness: u64,
    l2_size: u64,
}

impl Predefined2Record {
    pub(super) fn l2_fullness(&self) -> u64 {
        self.l2_fullness
    }

    pub(super) fn l2_size(&self) -> u64 {
        self.l2_size
    }
}

impl HasBlobFee for Predefined2Record {
    fn blob_fee_wei(&self) -> u64 {
        self.l1_blob_fee_wei
    }
}

pub(super) fn arb_l2_fullness_and_bytes_per_block(
    size: usize,
    capacity: u64,
) -> Vec<(u64, u32)> {
    let mut rng = StdRng::seed_from_u64(888);

    let fullness_noise: Vec<_> = std::iter::repeat(())
        .take(size)
        .map(|_| rng.gen_range(-0.5..0.5))
        // .map(|val| val * capacity as f64)
        .collect();

    const ROUGH_GAS_TO_BYTE_RATIO: f64 = 0.01;
    let bytes_scale: Vec<_> = std::iter::repeat(())
        .take(size)
        .map(|_| rng.gen_range(0.5..1.0))
        .map(|x| x * ROUGH_GAS_TO_BYTE_RATIO)
        .collect();

    (0usize..size)
        .map(|val| val as f64)
        .map(noisy_fullness)
        .map(|signal| (0.01 * signal + 0.01) * capacity as f64) // Scale and shift so it's between 0 and capacity
        .zip(fullness_noise)
        .map(|(fullness, noise)| fullness + noise)
        .map(|x| f64::min(x, capacity as f64))
        .map(|x| f64::max(x, 5.0))
        .zip(bytes_scale)
        .map(|(fullness, bytes_scale)| {
            let bytes = fullness * bytes_scale;
            (fullness, bytes)
        })
        .map(|(fullness, bytes)| (fullness as u64, std::cmp::max(bytes as u32, 1)))
        .collect()
}

fn noisy_fullness<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    const COMPONENTS: &[f64] = &[-30.0, 40.0, 700.0, -340.0, 400.0];
    let input = input.try_into().unwrap();
    gen_noisy_signal(input, COMPONENTS)
}
