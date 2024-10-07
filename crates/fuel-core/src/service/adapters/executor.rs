use crate::{
    database::RelayerIterableKeyValueView,
    service::adapters::TransactionsSource,
};
use fuel_core_executor::ports::MaybeCheckedTransaction;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::relayer::Event,
};

impl fuel_core_executor::ports::TransactionsSource for TransactionsSource {
    fn next(
        &self,
        gas_limit: u64,
        transactions_limit: u16,
        block_transaction_size_limit: u32,
        minimum_gas_price: u64,
    ) -> Vec<MaybeCheckedTransaction> {
        futures::executor::block_on(async {
            match self
                .txpool
                .select_transactions(
                    minimum_gas_price,
                    gas_limit,
                    transactions_limit,
                    block_transaction_size_limit,
                )
                .await
            {
                Ok(transactions) => transactions
                    .into_iter()
                    .map(|tx| {
                        MaybeCheckedTransaction::CheckedTransaction(
                            tx.as_ref().into(),
                            tx.used_consensus_parameters_version(),
                        )
                    })
                    .collect(),
                Err(_) => vec![],
            }
        })
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
