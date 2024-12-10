use fuel_core_upgradable_executor::native_executor::ports::{
    MaybeCheckedTransaction,
    TransactionsSource,
};
use std::num::NonZeroUsize;

pub struct OneCoreTxSource<TxSource> {
    source: TxSource,
    block_gas_limit: u64,
    gas_per_core: u64,
}

impl<TxSource> OneCoreTxSource<TxSource> {
    pub fn new(
        source: TxSource,
        block_gas_limit: u64,
        num_of_cores: NonZeroUsize,
    ) -> Self {
        let gas_per_core = block_gas_limit
            .checked_div(num_of_cores.get() as u64)
            .expect("The denominator is `NonZeroUsize`");

        Self {
            source,
            block_gas_limit,
            gas_per_core,
        }
    }
}

impl<TxSource> TransactionsSource for OneCoreTxSource<TxSource>
where
    TxSource: TransactionsSource,
{
    fn next(
        &self,
        gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u32,
    ) -> Vec<MaybeCheckedTransaction> {
        // Only gas for one core is available. Gas for other cores is not available.
        let non_available_cores_gas_limit =
            self.block_gas_limit.saturating_sub(self.gas_per_core);
        let core_gas_limit = gas_limit.saturating_sub(non_available_cores_gas_limit);

        self.source
            .next(core_gas_limit, tx_count_limit, block_transaction_size_limit)
    }
}
