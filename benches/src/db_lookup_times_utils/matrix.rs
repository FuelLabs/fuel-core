use fuel_core_types::fuel_types::BlockHeight;

pub const BLOCK_COUNT_MATRIX: [u32; 1] = [4];
pub const TX_COUNT_MATRIX: [u32; 1] = [1];

pub fn matrix() -> impl Iterator<Item = (BlockHeight, u32)> {
    BLOCK_COUNT_MATRIX.iter().flat_map(|&block_count| {
        TX_COUNT_MATRIX
            .iter()
            .map(move |&tx_count| (block_count.into(), tx_count))
    })
}
