use crate::ext;
use fuel_core_executor::ports::{
    MaybeCheckedTransaction,
    TransactionsSource,
};

pub struct WasmTxSource;

impl WasmTxSource {
    pub fn new() -> Self {
        Self
    }
}

impl TransactionsSource for WasmTxSource {
    fn next(&self, gas_limit: u64) -> Vec<MaybeCheckedTransaction> {
        ext::next_transactions(gas_limit).expect("Failed to get next transactions")
    }
}
