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
    BatchFeedbackHandle,
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
                total_gas: gas_limit,
                maximum_txs: transactions_limit,
                maximum_block_size: block_transaction_size_limit,
                excluded_contracts: HashSet::default(),
                execution_worker_count: 1,
                free_worker_count: 1,
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
        total_gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u64,
        selection_worker_count: usize,
        free_worker_count: usize,
        filter: Filter,
    ) -> anyhow::Result<TransactionSourceExecutableTransactions> {
        let (extracted_batches, excluded_contract_ids, ask_timings) = self
            .tx_pool
            .extract_transactions_for_block_async(Constraints {
                minimal_gas_price: self.minimum_gas_price,
                max_gas: gas_limit,
                total_gas: total_gas_limit,
                maximum_txs: tx_count_limit,
                maximum_block_size: block_transaction_size_limit,
                excluded_contracts: filter.excluded_contract_ids,
                execution_worker_count: selection_worker_count,
                free_worker_count,
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // Per-ask lifecycle checkpoints (in-memory counters; means = value /
        // parallel_executor_pool_asks): send->worker-start, in-pool, and the
        // return hop observed here. The executor-side "ready for workers"
        // stage is the existing prepare phase metric.
        {
            let m =
                fuel_core_metrics::parallel_executor_metrics::parallel_executor_metrics();
            m.pool_ask_queue_us.observe(ask_timings.queue_us as f64);
            m.pool_ask_in_pool_us.observe(ask_timings.in_pool_us as f64);
            m.pool_ask_return_us
                .observe(ask_timings.responded_at.elapsed().as_micros() as f64);
        }
        // The lane scheduler answers one ask with the COMPLETE worker
        // assignment (up to one batch per free worker), each carrying its
        // `BatchId`; the classic path answers with at most one id-less batch.
        let answered_all_workers = extracted_batches
            .iter()
            .any(|batch| batch.batch_id.is_some());
        let batches = extracted_batches
            .into_iter()
            .map(|batch| {
                let transactions = batch
                    .txs
                    .into_iter()
                    .map(|tx| {
                        let transaction = Arc::unwrap_or_clone(tx);
                        transaction.into()
                    })
                    .collect();
                // When the txpool lane scheduler answered this extraction it
                // assigned a `BatchId`; wrap it in an opaque handle that
                // reports the executor's measured timings straight back into
                // the pool. The executor never sees the batch id or the pool
                // channel — only the handle. When the lane scheduler is off,
                // `batch_id` is `None` and no handle is produced.
                let feedback_handle = batch.batch_id.map(|batch_id| {
                    let tx_pool = self.tx_pool.clone();
                    BatchFeedbackHandle::new(move |report| {
                        tx_pool.report_lane_scheduler_feedback(
                            batch_id,
                            report.execution_time,
                            report.overhead_time,
                            report.completed,
                        );
                    })
                });
                fuel_core_parallel_executor::ports::ExecutableBatch {
                    transactions,
                    anchor_contract_ids: batch.contracts,
                    feedback_handle,
                    // Production: contiguous indices from the scheduler's
                    // running counter (explicit indices are validation-only).
                    execution_indices: None,
                }
            })
            .collect();
        Ok(TransactionSourceExecutableTransactions {
            batches,
            filtered: TransactionFiltered::Filtered,
            filter: Filter {
                excluded_contract_ids,
            },
            answered_all_workers,
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
