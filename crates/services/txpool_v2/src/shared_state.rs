use std::{
    collections::HashSet,
    sync::Arc,
};

use crate::{
    Constraints,
    error::Error,
    lane_integration::{
        BatchFeedback,
        BatchId,
    },
    pool::TxPoolStats,
    pool_worker::{
        self,
        PoolReadRequest,
        PoolUpdateRequest,
    },
    service::{
        TxInfo,
        WritePoolRequest,
    },
};
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::ContractId,
    services::txpool::ArcPoolTx,
};
use tokio::sync::{
    mpsc,
    oneshot::{
        self,
        error::TryRecvError,
    },
    watch,
};

#[derive(Clone)]
pub struct SharedState {
    pub(crate) write_pool_requests_sender: mpsc::Sender<WritePoolRequest>,
    pub(crate) select_transactions_requests_sender:
        mpsc::Sender<pool_worker::PoolExtractBlockTransactions>,
    pub(crate) request_read_sender: mpsc::Sender<PoolReadRequest>,
    /// Update-request channel, cloned from the pool worker. Carries
    /// lane-scheduler batch feedback (and shares the pool worker's update queue
    /// with block processing / expiry).
    pub(crate) request_update_sender: mpsc::Sender<PoolUpdateRequest>,
    pub(crate) new_executable_txs_notifier: tokio::sync::watch::Sender<()>,
    pub(crate) latest_stats: tokio::sync::watch::Receiver<TxPoolStats>,
}

impl SharedState {
    pub fn try_insert(&self, transactions: Vec<Transaction>) -> Result<(), Error> {
        let transactions = transactions.into_iter().map(Arc::new).collect();
        self.write_pool_requests_sender
            .try_send(WritePoolRequest::InsertTxs { transactions })
            .map_err(|_| Error::ServiceQueueFull)?;

        Ok(())
    }

    pub async fn insert(&self, transaction: Transaction) -> Result<(), Error> {
        let transaction = Arc::new(transaction);
        let (sender, receiver) = oneshot::channel();

        self.write_pool_requests_sender
            .send(WritePoolRequest::InsertTx {
                transaction,
                response_channel: sender,
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?
    }

    /// This function has a hot loop inside to acquire transactions for the execution.
    /// It relies on the prioritization of the `TxPool`
    /// (it always tries to prioritize the `extract` call over other calls).
    /// In the future, extraction will be an async function,
    /// and we can remove this loop and just `await`.
    pub fn extract_transactions_for_block(
        &self,
        constraints: Constraints,
    ) -> Result<Vec<ArcPoolTx>, Error> {
        let (select_transactions_sender, mut select_transactions_receiver) =
            oneshot::channel();
        self.select_transactions_requests_sender
            .try_send(
                pool_worker::PoolExtractBlockTransactions::ExtractBlockTransactions {
                    constraints,
                    transactions: select_transactions_sender,
                },
            )
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        loop {
            let result = select_transactions_receiver.try_recv();
            match result {
                Ok((txs, _, _, _)) => {
                    return Ok(txs);
                }
                Err(TryRecvError::Empty) => continue,
                Err(TryRecvError::Closed) => {
                    return Err(Error::ServiceCommunicationFailed);
                }
            }
        }
    }

    pub async fn extract_transactions_for_block_async(
        &self,
        constraints: Constraints,
    ) -> Result<
        (
            Vec<ArcPoolTx>,
            HashSet<ContractId>,
            Vec<ContractId>,
            Option<BatchId>,
        ),
        Error,
    > {
        let (select_transactions_sender, select_transactions_receiver) =
            oneshot::channel();
        self.select_transactions_requests_sender
            .try_send(
                pool_worker::PoolExtractBlockTransactions::ExtractBlockTransactions {
                    constraints,
                    transactions: select_transactions_sender,
                },
            )
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        select_transactions_receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)
    }

    /// Report lane-scheduler batch-completion feedback for the batch identified
    /// by `batch_id` (assigned by [`Self::extract_transactions_for_block_async`]).
    ///
    /// This is the executor→pool half of the feedback loop: the executor calls
    /// it once a batch's execution result is registered. The send is
    /// non-blocking and best-effort — if the queue is full or the pool worker
    /// has shut down the feedback is silently dropped, which the lane scheduler
    /// tolerates by design (missing feedback only stops the adaptive slice from
    /// adapting; correctness is unaffected). No-op transport-wise when the lane
    /// scheduler is disabled (no batch id is ever produced, so this is never
    /// called on that path).
    pub fn report_lane_scheduler_feedback(
        &self,
        batch_id: BatchId,
        execution_time: u64,
        overhead_time: u64,
        completed: bool,
    ) {
        let feedback = BatchFeedback {
            batch_id,
            execution_time,
            overhead_time,
            completed,
        };
        // Best-effort: ignore a full queue or a closed receiver (shutdown).
        let _ = self.request_update_sender.try_send(
            PoolUpdateRequest::LaneSchedulerFeedback {
                feedback: vec![feedback],
            },
        );
    }

    pub async fn get_tx_ids(&self, max_txs: usize) -> Result<Vec<TxId>, Error> {
        let (response_channel, result_receiver) = oneshot::channel();

        self.request_read_sender
            .send(PoolReadRequest::TxIds {
                max_txs,
                response_channel,
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        result_receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)
    }

    pub async fn find_one(&self, tx_id: TxId) -> Result<Option<TxInfo>, Error> {
        Ok(self.find(vec![tx_id]).await?.pop().flatten())
    }

    pub async fn find(&self, tx_ids: Vec<TxId>) -> Result<Vec<Option<TxInfo>>, Error> {
        let (response_channel, result_receiver) = oneshot::channel();

        self.request_read_sender
            .send(PoolReadRequest::Txs {
                tx_ids,
                response_channel,
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        result_receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)
    }

    /// Get a notifier that is notified when new executable transactions are added to the pool.
    pub fn get_new_executable_txs_notifier(&self) -> watch::Receiver<()> {
        self.new_executable_txs_notifier.subscribe()
    }

    pub fn latest_stats(&self) -> TxPoolStats {
        *self.latest_stats.borrow()
    }
}
