use crate::{
    database::{
        transaction::OwnedTransactionIndexCursor,
        Database,
    },
    query::{
        transaction_status_change,
        TxnStatusChangeState,
    },
    schema::scalars::{
        Address,
        HexString,
        SortedTxCursor,
        TransactionId,
    },
    state::IterDirection,
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
use fuel_core_storage::{
    not_found,
    tables::{
        FuelBlocks,
        Transactions,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_txpool::Service as TxPoolService;
use fuel_core_types::{
    fuel_tx::{
        Cacheable,
        Transaction as FuelTx,
    },
    fuel_types,
    fuel_types::bytes::Deserializable,
};
use futures::{
    Stream,
    StreamExt,
    TryStreamExt,
};
use itertools::Itertools;
use std::{
    iter,
    ops::Deref,
    sync::Arc,
};
use tokio_stream::wrappers::BroadcastStream;
use types::Transaction;

use self::types::TransactionStatus;

pub mod input;
pub mod output;
pub mod receipt;
pub mod types;

#[derive(Default)]
pub struct TxQuery;

#[Object]
impl TxQuery {
    async fn transaction(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The ID of the transaction")] id: TransactionId,
    ) -> async_graphql::Result<Option<Transaction>> {
        let db = ctx.data_unchecked::<Database>();
        let id = id.0;
        let txpool = ctx.data_unchecked::<Arc<TxPoolService>>();

        if let Ok(Some(transaction)) = txpool.find_one(id).await {
            Ok(Some(Transaction(transaction.tx().clone().deref().into())))
        } else {
            Ok(db
                .storage::<Transactions>()
                .get(&id)?
                .map(|tx| Transaction(tx.into_owned())))
        }
    }

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
        let db = ctx.data_unchecked::<Database>();
        crate::schema::query_pagination(
            after,
            before,
            first,
            last,
            |start: &Option<SortedTxCursor>, direction| {
                let start = *start;
                let block_id = start.map(|sorted| sorted.block_height);
                let all_block_ids = db.all_block_ids(block_id, Some(direction));

                let all_txs = all_block_ids
                    .flat_map(move |block| {
                        block.map_err(StorageError::from).map(
                            |(block_height, block_id)| {
                                db.storage::<FuelBlocks>()
                                    .get(&block_id)
                                    .transpose()
                                    .ok_or(not_found!(FuelBlocks))?
                                    .map(|fuel_block| {
                                        let mut txs =
                                            fuel_block.into_owned().into_inner().1;

                                        if direction == IterDirection::Reverse {
                                            txs.reverse();
                                        }

                                        txs.into_iter().zip(iter::repeat(block_height))
                                    })
                            },
                        )
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
                        let tx = db
                            .storage::<Transactions>()
                            .get(&sorted.tx_id.0)
                            .transpose()
                            .ok_or(not_found!(Transactions))??
                            .into_owned();

                        Ok((sorted, tx.into()))
                    })
                });

                Ok(all_txs)
            },
        )
        .await
    }

    async fn transactions_by_owner(
        &self,
        ctx: &Context<'_>,
        owner: Address,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<HexString, Transaction, EmptyFields, EmptyFields>>
    {
        let db = ctx.data_unchecked::<Database>();
        let owner = fuel_types::Address::from(owner);

        crate::schema::query_pagination(
            after,
            before,
            first,
            last,
            |start: &Option<HexString>, direction| {
                let start: Option<OwnedTransactionIndexCursor> =
                    start.clone().map(Into::into);
                let txs = db
                    .owned_transactions(&owner, start.as_ref(), Some(direction))
                    .map(|result| {
                        result
                            .map_err(StorageError::from)
                            .and_then(|(cursor, tx_id)| {
                                let tx = db
                                    .storage::<Transactions>()
                                    .get(&tx_id)?
                                    .ok_or(not_found!(Transactions))?
                                    .into_owned();
                                Ok((cursor.into(), tx.into()))
                            })
                    });
                Ok(txs)
            },
        )
        .await
    }
}

#[derive(Default)]
pub struct TxMutation;

#[Object]
impl TxMutation {
    /// Execute a dry-run of the transaction using a fork of current state, no changes are committed.
    async fn dry_run(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
        // If set to false, disable input utxo validation, overriding the configuration of the node.
        // This allows for non-existent inputs to be used without signature validation
        // for read-only calls.
        utxo_validation: Option<bool>,
    ) -> async_graphql::Result<Vec<receipt::Receipt>> {
        let block_producer =
            ctx.data_unchecked::<Arc<fuel_core_producer::Producer<Database>>>();

        let mut tx = FuelTx::from_bytes(&tx.0)?;
        tx.precompute();

        let receipts = block_producer.dry_run(tx, None, utxo_validation).await?;
        Ok(receipts.iter().map(Into::into).collect())
    }

    /// Submits transaction to the txpool
    async fn submit(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<Transaction> {
        let txpool = ctx.data_unchecked::<Arc<TxPoolService>>();
        let mut tx = FuelTx::from_bytes(&tx.0)?;
        tx.precompute();
        let _: Vec<_> = txpool
            .insert(vec![Arc::new(tx.clone())])
            .await?
            .into_iter()
            .try_collect()?;

        let tx = Transaction(tx);
        Ok(tx)
    }
}

#[derive(Default)]
pub struct TxStatusSubscription;

struct StreamState {
    txpool: Arc<TxPoolService>,
    db: Database,
}

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
    async fn status_change(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The ID of the transaction")] id: TransactionId,
    ) -> impl Stream<Item = async_graphql::Result<TransactionStatus>> {
        let txpool = ctx.data_unchecked::<Arc<TxPoolService>>().clone();
        let db = ctx.data_unchecked::<Database>().clone();
        let rx = BroadcastStream::new(txpool.tx_update_subscribe());
        let state = Box::new(StreamState { txpool, db });

        transaction_status_change(state, rx.boxed(), id.into())
            .await
            .map_err(async_graphql::Error::from)
    }
}

#[async_trait::async_trait]
impl TxnStatusChangeState for StreamState {
    async fn get_tx_status(
        &self,
        id: fuel_core_types::fuel_types::Bytes32,
    ) -> anyhow::Result<Option<TransactionStatus>> {
        Ok(types::get_tx_status(id, &self.db, &self.txpool)
            .await
            .map_err(|e| anyhow::anyhow!("Database lookup failed {:?}", e))?)
    }
}
