use crate::{
    database::RelayerIterableKeyValueView,
    service::adapters::TransactionsSource,
};
use fuel_core_executor::ports::MaybeCheckedTransaction;
use fuel_core_txpool::Constraints;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::relayer::Event,
};
use std::sync::Arc;

impl fuel_core_executor::ports::TransactionsSource for TransactionsSource {
    fn next(
        &self,
        gas_limit: u64,
        transactions_limit: u16,
        block_transaction_size_limit: u32,
    ) -> Vec<MaybeCheckedTransaction> {
        self.tx_pool
            .exclusive_lock()
            .extract_transactions_for_block(Constraints {
                minimal_gas_price: self.minimum_gas_price,
                max_gas: gas_limit,
                maximum_txs: transactions_limit,
                maximum_block_size: block_transaction_size_limit,
            })
            .into_iter()
            .map(|tx| {
                let transaction = Arc::unwrap_or_clone(tx);
                let version = transaction.used_consensus_parameters_version();
                MaybeCheckedTransaction::CheckedTransaction(transaction.into(), version)
            })
            .collect()
    }
}

impl fuel_core_executor::ports::RelayerPort for RelayerIterableKeyValueView {
    fn enabled(&self) -> bool {
        #[cfg(feature = "relayer")]
        {
            true
        }
        #[cfg(not(feature = "relayer"))]
        {
            false
        }
    }

    fn get_events(&self, da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
        #[cfg(feature = "relayer")]
        {
            use fuel_core_storage::StorageAsRef;
            let events = self
                .storage::<fuel_core_relayer::storage::EventsHistory>()
                .get(da_height)?
                .map(|cow| cow.into_owned())
                .unwrap_or_default();
            Ok(events)
        }
        #[cfg(not(feature = "relayer"))]
        {
            let _ = da_height;
            Ok(vec![])
        }
    }
}
