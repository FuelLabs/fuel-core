pub const BLOCK_COUNT_MATRIX: [u32; 1] = [1];
pub const TX_COUNT_MATRIX: [u32; 1] = [1];

// todo: we can make this lazy loaded
pub fn matrix() -> Box<dyn Iterator<Item = (u32, u32)>> {
    let iter = BLOCK_COUNT_MATRIX.iter().flat_map(|&block_count| {
        TX_COUNT_MATRIX
            .iter()
            .map(move |&tx_count| (block_count, tx_count))
    });

    Box::new(iter)
}
