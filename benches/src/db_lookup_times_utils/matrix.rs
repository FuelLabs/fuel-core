use fuel_core_types::fuel_types::BlockHeight;

pub const BLOCK_COUNT_MATRIX: [u32; 1] = [4];
pub const TX_COUNT_MATRIX: [u32; 1] = [1];

pub const DB_CLEAN_UP: bool = true;

pub fn should_clean() -> bool {
    // Override cleaning up databases if env var is set
    std::env::var_os("DB_CLEAN_UP")
        .map(|value| {
            let value = value.to_str().unwrap();
            let value = value.parse::<bool>().unwrap();
            println!("DB cleanup enabled: {}", value);
            value
        })
        .unwrap_or(DB_CLEAN_UP)
}

pub fn matrix() -> impl Iterator<Item = (BlockHeight, u32)> {
    BLOCK_COUNT_MATRIX.iter().flat_map(|&block_count| {
        TX_COUNT_MATRIX
            .iter()
            .map(move |&tx_count| (block_count.into(), tx_count))
    })
}
