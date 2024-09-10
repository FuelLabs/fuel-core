use crate::{
    fuel_core_graphql_api::{
        api_service::{
            BlockProducer,
            ConsensusProvider,
            TxPool,
        },
        ports::OffChainDatabase,
        IntoApiResult,
        QUERY_COSTS,
    },
    query::{
        transaction_status_change,
        BlockQueryData,
        SimpleTransactionData,
        TransactionQueryData,
    },
    schema::{
        scalars::{
            Address,
            HexString,
            SortedTxCursor,
            TransactionId,
            TxPointer,
        },
        tx::types::TransactionStatus,
        ReadViewProvider,
    },
    service::adapters::SharedMemoryPool,
};
use async_graphql::{
    connection::{
        Connection,
        EmptyFields,
    },
    Context,
    Object,
    Subscription,
};
use fuel_core_executor::ports::TransactionExt;
use fuel_core_storage::{
    iter::IterDirection,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_txpool::{
    ports::MemoryPool,
    service::TxStatusMessage,
};
use fuel_core_types::{
    fuel_tx::{
        Cacheable,
        Transaction as FuelTx,
        UniqueIdentifier,
    },
    fuel_types::{
        self,
        canonical::Deserialize,
    },
    fuel_vm::checked_transaction::{
        CheckPredicateParams,
        EstimatePredicates,
    },
    services::txpool,
};
use futures::{
    Stream,
    TryStreamExt,
};
use itertools::Itertools;
use std::{
    iter,
    sync::Arc,
};
use tokio_stream::StreamExt;
use types::{
    DryRunTransactionExecutionStatus,
    Transaction,
};

use super::scalars::U64;

pub mod input;
pub mod output;
pub mod receipt;
pub mod types;
pub mod upgrade_purpose;

#[derive(Default)]
pub struct TxQuery;

#[Object]
impl TxQuery {
    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn transaction(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The ID of the transaction")] id: TransactionId,
    ) -> async_graphql::Result<Option<Transaction>> {
        let query = ctx.read_view()?;
        let id = id.0;
        let txpool = ctx.data_unchecked::<TxPool>();

        if let Some(transaction) = txpool.transaction(id) {
            Ok(Some(Transaction(transaction, id)))
        } else {
            query
                .transaction(&id)
                .map(|tx| Transaction::from_tx(id, tx))
                .into_api_result()
        }
    }

    #[graphql(complexity = "{\
        QUERY_COSTS.storage_iterator\
        + (QUERY_COSTS.storage_read + first.unwrap_or_default() as usize) * child_complexity \
        + (QUERY_COSTS.storage_read + last.unwrap_or_default() as usize) * child_complexity\
    }")]
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<
        Connection<SortedTxCursor, Transaction, EmptyFields, EmptyFields>,
    > {
        let query = ctx.read_view()?;
        crate::schema::query_pagination(
            after,
            before,
            first,
            last,
            |start: &Option<SortedTxCursor>, direction| {
                let start = *start;
                let block_id = start.map(|sorted| sorted.block_height);
                let all_block_ids = query.compressed_blocks(block_id, direction);

                let all_txs = all_block_ids
                    .map(move |block| {
                        block.map(|fuel_block| {
                            let (header, mut txs) = fuel_block.into_inner();

                            if direction == IterDirection::Reverse {
                                txs.reverse();
                            }

                            txs.into_iter().zip(iter::repeat(*header.height()))
                        })
                    })
                    .flatten_ok()
                    .map(|result| {
                        result.map(|(tx_id, block_height)| {
                            SortedTxCursor::new(block_height, tx_id.into())
                        })
                    })
                    .skip_while(move |result| {
                        if let Ok(sorted) = result {
                            if let Some(start) = start {
                                return sorted != &start
                            }
                        }
                        false
                    });
                let all_txs = all_txs.map(|result: StorageResult<SortedTxCursor>| {
                    result.and_then(|sorted| {
                        let tx = query.transaction(&sorted.tx_id.0)?;

                        Ok((sorted, Transaction::from_tx(sorted.tx_id.0, tx)))
                    })
                });

                Ok(all_txs)
            },
        )
        .await
    }

    #[graphql(complexity = "{\
        QUERY_COSTS.storage_iterator\
        + (QUERY_COSTS.storage_read + first.unwrap_or_default() as usize) * child_complexity \
        + (QUERY_COSTS.storage_read + last.unwrap_or_default() as usize) * child_complexity\
    }")]
    async fn transactions_by_owner(
        &self,
        ctx: &Context<'_>,
        owner: Address,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<TxPointer, Transaction, EmptyFields, EmptyFields>>
    {
        let query = ctx.read_view()?;
        let params = ctx
            .data_unchecked::<ConsensusProvider>()
            .latest_consensus_params();
        let owner = fuel_types::Address::from(owner);

        crate::schema::query_pagination(
            after,
            before,
            first,
            last,
            |start: &Option<TxPointer>, direction| {
                let start = (*start).map(Into::into);
                let txs =
                    query
                        .owned_transactions(owner, start, direction)
                        .map(|result| {
                            result.map(|(cursor, tx)| {
                                let tx_id = tx.id(&params.chain_id());
                                (cursor.into(), Transaction::from_tx(tx_id, tx))
                            })
                        });
                Ok(txs)
            },
        )
        .await
    }

    /// Estimate the predicate gas for the provided transaction
    #[graphql(complexity = "QUERY_COSTS.estimate_predicates + child_complexity")]
    async fn estimate_predicates(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<Transaction> {
        let mut tx = FuelTx::from_bytes(&tx.0)?;

        let params = ctx
            .data_unchecked::<ConsensusProvider>()
            .latest_consensus_params();

        let memory_pool = ctx.data_unchecked::<SharedMemoryPool>();
        let memory = memory_pool.get_memory().await;

        let parameters = CheckPredicateParams::from(params.as_ref());
        let tx = tokio_rayon::spawn_fifo(move || {
            let result = tx.estimate_predicates(&parameters, memory);
            result.map(|_| tx)
        })
        .await
        .map_err(|err| anyhow::anyhow!("{:?}", err))?;

        Ok(Transaction::from_tx(tx.id(&params.chain_id()), tx))
    }

    #[cfg(feature = "test-helpers")]
    /// Returns all possible receipts for test purposes.
    async fn all_receipts(&self) -> Vec<receipt::Receipt> {
        receipt::all_receipts()
            .into_iter()
            .map(Into::into)
            .collect()
    }
}

#[derive(Default)]
pub struct TxMutation;

#[Object]
impl TxMutation {
    /// Execute a dry-run of multiple transactions using a fork of current state, no changes are committed.
    #[graphql(
        complexity = "QUERY_COSTS.dry_run * txs.len() + child_complexity * txs.len()"
    )]
    async fn dry_run(
        &self,
        ctx: &Context<'_>,
        txs: Vec<HexString>,
        // If set to false, disable input utxo validation, overriding the configuration of the node.
        // This allows for non-existent inputs to be used without signature validation
        // for read-only calls.
        utxo_validation: Option<bool>,
        gas_price: Option<U64>,
    ) -> async_graphql::Result<Vec<DryRunTransactionExecutionStatus>> {
        let block_producer = ctx.data_unchecked::<BlockProducer>();
        let consensus_params = ctx
            .data_unchecked::<ConsensusProvider>()
            .latest_consensus_params();
        let block_gas_limit = consensus_params.block_gas_limit();

        let mut transactions = txs
            .iter()
            .map(|tx| FuelTx::from_bytes(&tx.0))
            .collect::<Result<Vec<FuelTx>, _>>()?;
        transactions.iter_mut().try_fold::<_, _, async_graphql::Result<u64>>(0u64, |acc, tx| {
            let gas = tx.max_gas(&consensus_params)?;
            let gas = gas.saturating_add(acc);
            if gas > block_gas_limit {
                return Err(anyhow::anyhow!("The sum of the gas usable by the transactions is greater than the block gas limit").into());
            }
            tx.precompute(&consensus_params.chain_id())?;
            Ok(gas)
        })?;

        let tx_statuses = block_producer
            .dry_run_txs(
                transactions,
                None,
                utxo_validation,
                gas_price.map(|x| x.into()),
            )
            .await?;
        let tx_statuses = tx_statuses
            .into_iter()
            .map(DryRunTransactionExecutionStatus)
            .collect();

        Ok(tx_statuses)
    }

    /// Submits transaction to the `TxPool`.
    ///
    /// Returns submitted transaction if the transaction is included in the `TxPool` without problems.
    #[graphql(complexity = "QUERY_COSTS.submit + child_complexity")]
    async fn submit(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<Transaction> {
        let txpool = ctx.data_unchecked::<TxPool>();
        let params = ctx
            .data_unchecked::<ConsensusProvider>()
            .latest_consensus_params();
        let tx = FuelTx::from_bytes(&tx.0)?;

        let _: Vec<_> = txpool
            .insert(vec![Arc::new(tx.clone())])
            .await
            .into_iter()
            .try_collect()?;
        let id = tx.id(&params.chain_id());

        let tx = Transaction(tx, id);
        Ok(tx)
    }
}

#[derive(Default)]
pub struct TxStatusSubscription;

#[Subscription]
impl TxStatusSubscription {
    /// Returns a stream of status updates for the given transaction id.
    /// If the current status is [`TransactionStatus::Success`], [`TransactionStatus::SqueezedOut`]
    /// or [`TransactionStatus::Failed`] the stream will return that and end immediately.
    /// If the current status is [`TransactionStatus::Submitted`] this will be returned
    /// and the stream will wait for a future update.
    ///
    /// This stream will wait forever so it's advised to use within a timeout.
    ///
    /// It is possible for the stream to miss an update if it is polled slower
    /// then the updates arrive. In such a case the stream will close without
    /// a status. If this occurs the stream can simply be restarted to return
    /// the latest status.
    #[graphql(complexity = "QUERY_COSTS.status_change + child_complexity")]
    async fn status_change<'a>(
        &self,
        ctx: &'a Context<'a>,
        #[graphql(desc = "The ID of the transaction")] id: TransactionId,
    ) -> anyhow::Result<impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a>
    {
        let txpool = ctx.data_unchecked::<TxPool>();
        let rx = txpool.tx_update_subscribe(id.into())?;
        let query = ctx.read_view()?;

        Ok(transaction_status_change(
            move |id| match query.tx_status(&id) {
                Ok(status) => Ok(Some(status)),
                Err(StorageError::NotFound(_, _)) => Ok(txpool
                    .submission_time(id)
                    .map(|time| txpool::TransactionStatus::Submitted { time })),
                Err(err) => Err(err),
            },
            rx,
            id.into(),
        )
        .map_err(async_graphql::Error::from))
    }

    /// Submits transaction to the `TxPool` and await either confirmation or failure.
    #[graphql(complexity = "QUERY_COSTS.submit_and_await + child_complexity")]
    async fn submit_and_await<'a>(
        &self,
        ctx: &'a Context<'a>,
        tx: HexString,
    ) -> async_graphql::Result<
        impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a,
    > {
        let subscription = submit_and_await_status(ctx, tx).await?;

        Ok(subscription
            .skip_while(|event| matches!(event, Ok(TransactionStatus::Submitted(..))))
            .take(1))
    }

    /// Submits the transaction to the `TxPool` and returns a stream of events.
    /// Compared to the `submitAndAwait`, the stream also contains `
    /// SubmittedStatus` as an intermediate state.
    #[graphql(complexity = "QUERY_COSTS.submit_and_await + child_complexity")]
    async fn submit_and_await_status<'a>(
        &self,
        ctx: &'a Context<'a>,
        tx: HexString,
    ) -> async_graphql::Result<
        impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a,
    > {
        submit_and_await_status(ctx, tx).await
    }
}

async fn submit_and_await_status<'a>(
    ctx: &'a Context<'a>,
    tx: HexString,
) -> async_graphql::Result<
    impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a,
> {
    let txpool = ctx.data_unchecked::<TxPool>();
    let params = ctx
        .data_unchecked::<ConsensusProvider>()
        .latest_consensus_params();
    let tx = FuelTx::from_bytes(&tx.0)?;
    let tx_id = tx.id(&params.chain_id());
    let subscription = txpool.tx_update_subscribe(tx_id)?;

    let _: Vec<_> = txpool
        .insert(vec![Arc::new(tx)])
        .await
        .into_iter()
        .try_collect()?;

    Ok(subscription
        .map(move |event| match event {
            TxStatusMessage::Status(status) => {
                let status = TransactionStatus::new(tx_id, status);
                Ok(status)
            }
            TxStatusMessage::FailedStatus => {
                Err(anyhow::anyhow!("Failed to get transaction status").into())
            }
        })
        .take(2))
}
