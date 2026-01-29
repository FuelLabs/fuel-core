use super::PreconfirmationSender;
use crate::{
    database::{
        RegularStage,
        RelayerIterableKeyValueView,
        database_description::relayer::Relayer,
    },
    service::adapters::{
        NewTxWaiter,
        TransactionsSource,
    },
    state::{
        data_source::DataSource,
        generic_database::GenericDatabase,
    },
};
use fuel_core_executor::{
    executor::WaitNewTransactionsResult,
    ports::{
        MaybeCheckedTransaction,
        NewTxWaiterPort,
        PreconfirmationSenderPort,
    },
};
#[cfg(feature = "parallel-executor")]
use fuel_core_parallel_executor::ports::{
    Filter,
    TransactionFiltered,
    TransactionSourceExecutableTransactions,
};
use fuel_core_txpool::Constraints;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::{
        preconfirmation::Preconfirmation,
        relayer::Event,
    },
};
use std::{
    collections::HashSet,
    sync::Arc,
};
use tokio::sync::mpsc::error::TrySendError;

impl fuel_core_executor::ports::TransactionsSource for TransactionsSource {
    fn next(
        &self,
        gas_limit: u64,
        #[cfg(not(feature = "u32-tx-count"))] transactions_limit: u16,
        #[cfg(feature = "u32-tx-count")] transactions_limit: u32,
        block_transaction_size_limit: u64,
    ) -> Vec<MaybeCheckedTransaction> {
        self.tx_pool
            .extract_transactions_for_block(Constraints {
                minimal_gas_price: self.minimum_gas_price,
                max_gas: gas_limit,
                maximum_txs: transactions_limit,
                maximum_block_size: block_transaction_size_limit,
                excluded_contracts: HashSet::default(),
            })
            .unwrap_or_default()
            .into_iter()
            .map(|tx| {
                let transaction = Arc::unwrap_or_clone(tx);
                let version = transaction.used_consensus_parameters_version();
                MaybeCheckedTransaction::CheckedTransaction(transaction.into(), version)
            })
            .collect()
    }
}

#[cfg(feature = "parallel-executor")]
impl fuel_core_parallel_executor::ports::TransactionsSource for TransactionsSource {
    async fn get_executable_transactions(
        &self,
        gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u64,
        filter: Filter,
    ) -> anyhow::Result<TransactionSourceExecutableTransactions> {
        let (transactions, excluded_contract_ids) = self
            .tx_pool
            .extract_transactions_for_block_async(Constraints {
                minimal_gas_price: self.minimum_gas_price,
                max_gas: gas_limit,
                maximum_txs: tx_count_limit,
                maximum_block_size: block_transaction_size_limit,
                excluded_contracts: filter.excluded_contract_ids,
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let transactions = transactions
            .into_iter()
            .map(|tx| {
                let transaction = Arc::unwrap_or_clone(tx);
                transaction.into()
            })
            .collect();
        Ok(TransactionSourceExecutableTransactions {
            transactions,
            filtered: TransactionFiltered::Filtered,
            filter: Filter {
                excluded_contract_ids,
            },
        })
    }

    fn get_new_transactions_notifier(&self) -> tokio::sync::watch::Receiver<()> {
        self.tx_pool.get_new_executable_txs_notifier()
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

impl fuel_core_executor::ports::RelayerPort
    for GenericDatabase<DataSource<Relayer, RegularStage<Relayer>>, std::io::Empty>
{
    fn enabled(&self) -> bool {
        todo!()
    }

    fn get_events(&self, _da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
        todo!()
    }
}

impl NewTxWaiterPort for NewTxWaiter {
    async fn wait_for_new_transactions(&mut self) -> WaitNewTransactionsResult {
        tokio::select! {
            _ = tokio::time::sleep_until(self.timeout) => {
                WaitNewTransactionsResult::Timeout
            }
            res = self.receiver.changed() => {
                match res {
                    Ok(_) => {
                        WaitNewTransactionsResult::NewTransaction
                    }
                    Err(_) => {
                        WaitNewTransactionsResult::Timeout
                    }
                }
            }
        }
    }
}

impl PreconfirmationSenderPort for PreconfirmationSender {
    async fn send(&self, preconfirmations: Vec<Preconfirmation>) {
        // TODO: Avoid cloning of the `preconfirmations`
        self.tx_status_manager_adapter
            .tx_status_manager_shared_data
            .update_preconfirmations(preconfirmations.clone());

        // If the receiver is closed, it means no one is listening to the preconfirmations and so we can drop them.
        // We don't consider this an error.
        let _ = self.sender_signature_service.send(preconfirmations).await;
    }

    fn try_send(&self, preconfirmations: Vec<Preconfirmation>) -> Vec<Preconfirmation> {
        match self.sender_signature_service.try_reserve() {
            Ok(permit) => {
                // TODO: Avoid cloning of the `preconfirmations`
                self.tx_status_manager_adapter
                    .tx_status_manager_shared_data
                    .update_preconfirmations(preconfirmations.clone());
                permit.send(preconfirmations);
                vec![]
            }
            // If the receiver is closed, it means no one is listening to the preconfirmations and so we can drop them.
            // We don't consider this an error.
            Err(TrySendError::Closed(_)) => {
                self.tx_status_manager_adapter
                    .tx_status_manager_shared_data
                    .update_preconfirmations(preconfirmations);
                vec![]
            }
            Err(TrySendError::Full(_)) => preconfirmations,
        }
    }
}
