use crate::{
    database::RelayerIterableKeyValueView,
    service::adapters::{
        NewTxWaiter,
        TransactionsSource,
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
use fuel_core_txpool::Constraints;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::{
        preconfirmation::PreconfirmationStatus,
        relayer::Event,
    },
};
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;

use super::PreconfirmationSender;

impl fuel_core_executor::ports::TransactionsSource for TransactionsSource {
    fn next(
        &self,
        gas_limit: u64,
        transactions_limit: u16,
        block_transaction_size_limit: u32,
    ) -> Vec<MaybeCheckedTransaction> {
        self.tx_pool
            .extract_transactions_for_block(Constraints {
                minimal_gas_price: self.minimum_gas_price,
                max_gas: gas_limit,
                maximum_txs: transactions_limit,
                maximum_block_size: block_transaction_size_limit,
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
    async fn send(&self, preconfirmations: Vec<PreconfirmationStatus>) {
        // If the receiver is closed, it means no one is listening to the preconfirmations and so we can drop them.
        // We don't consider this an error.
        let _ = self.sender.send(preconfirmations).await;
    }

    fn try_send(
        &self,
        preconfirmations: Vec<PreconfirmationStatus>,
    ) -> Vec<PreconfirmationStatus> {
        match self.sender.try_send(preconfirmations) {
            Ok(()) => vec![],
            // If the receiver is closed, it means no one is listening to the preconfirmations and so we can drop them.
            // We don't consider this an error.
            Err(TrySendError::Closed(_)) => vec![],
            Err(TrySendError::Full(preconfirmations)) => preconfirmations,
        }
    }
}
