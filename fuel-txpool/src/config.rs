#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum number of transactions inside the pool
    pub max_tx: usize,
    /// max depth of connected UTXO excluding contracts
    pub max_depth: usize,
    /// The minimum allowed gas price
    pub min_gas_price: u64,
    /// The minimum allowed byte price
    pub min_byte_price: u64,
    /// If non-zero, the tx pool will require the byte price to be
    /// _at least_ this fraction of the gas price, `byte_price >= gas_price / max_gas_per_byte`.
    /// This protects against front-runners attempting to manipulating a high gas price on
    /// transactions that don't incur any execution costs, since the tx pool only
    /// prioritizes gas prices.
    pub max_gas_per_byte_price: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_tx: 4064,
            max_depth: 10,
            min_gas_price: 0,
            min_byte_price: 0,
            max_gas_per_byte_price: 0,
        }
    }
}
