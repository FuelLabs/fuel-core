pub const BLOCK_COUNT_MATRIX: [u32; 1] = [1];
pub const TX_COUNT_MATRIX: [u32; 1] = [1];

pub fn matrix() -> impl Iterator<Item = (u32, u32)> {
    BLOCK_COUNT_MATRIX.iter().flat_map(|&block_count| {
        TX_COUNT_MATRIX
            .iter()
            .map(move |&tx_count| (block_count, tx_count))
    })
}
