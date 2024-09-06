use fuel_core_types::fuel_types::BlockHeight;

pub const BLOCK_COUNT_MATRIX: [u32; 5] = [10, 100, 1000, 5000, 10_000];
pub const TX_COUNT_MATRIX: [u32; 3] = [500, 1000, 2000];

pub fn matrix() -> impl Iterator<Item = (BlockHeight, u32)> {
    BLOCK_COUNT_MATRIX.iter().flat_map(|&block_count| {
        TX_COUNT_MATRIX
            .iter()
            .map(move |&tx_count| (block_count.into(), tx_count))
    })
}
