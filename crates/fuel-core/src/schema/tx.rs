use crate::{
    fuel_core_graphql_api::{
        service::{
            BlockProducer,
            Database,
            TxPool,
        },
        IntoApiResult,
    },
    graphql_api::Config,
    query::{
        transaction_status_change,
        BlockQueryData,
        SimpleTransactionData,
        TransactionQueryData,
        TxnStatusChangeState,
    },
    schema::scalars::{
        Address,
        HexString,
        SortedTxCursor,
        TransactionId,
        TxPointer,
    },
};
use anyhow::anyhow;
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
    iter::IterDirection,
    Result as StorageResult,
};
use fuel_core_types::{
    fuel_tx::{
        Cacheable,
        Transaction as FuelTx,
        UniqueIdentifier,
    },
    fuel_types,
    fuel_types::bytes::Deserializable,
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
        let query: &Database = ctx.data_unchecked();
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
        let db_query: &Database = ctx.data_unchecked();
        let tx_query: &Database = ctx.data_unchecked();
        crate::schema::query_pagination(
            after,
            before,
            first,
            last,
            |start: &Option<SortedTxCursor>, direction| {
                let start = *start;
                let block_id = start.map(|sorted| sorted.block_height);
                let all_block_ids = db_query.compressed_blocks(block_id, direction);

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
                        let tx = tx_query.transaction(&sorted.tx_id.0)?;

                        Ok((sorted, Transaction::from_tx(sorted.tx_id.0, tx)))
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
    ) -> async_graphql::Result<Connection<TxPointer, Transaction, EmptyFields, EmptyFields>>
    {
        // Rocksdb doesn't support reverse iteration over a prefix
        if matches!(last, Some(last) if last > 0) {
            return Err(
                anyhow!("reverse pagination isn't supported for this resource").into(),
            )
        }

        let query: &Database = ctx.data_unchecked();
        let config = ctx.data_unchecked::<Config>();
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
                                let tx_id = tx.id(&config.transaction_parameters);
                                (cursor.into(), Transaction::from_tx(tx_id, tx))
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
        let block_producer = ctx.data_unchecked::<BlockProducer>();
        let config = ctx.data_unchecked::<Config>();

        let mut tx = FuelTx::from_bytes(&tx.0)?;
        tx.precompute(&config.transaction_parameters);

        let receipts = block_producer.dry_run_tx(tx, None, utxo_validation).await?;
        Ok(receipts.iter().map(Into::into).collect())
    }

    /// Submits transaction to the txpool
    async fn submit(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<Transaction> {
        let txpool = ctx.data_unchecked::<TxPool>();
        let config = ctx.data_unchecked::<Config>();
        let mut tx = FuelTx::from_bytes(&tx.0)?;
        tx.precompute(&config.transaction_parameters);
        // TODO: use spawn_blocking here
        let _: Vec<_> = txpool
            .insert(vec![Arc::new(tx.clone())])
            .into_iter()
            .try_collect()?;
        let id = tx.id(&config.transaction_parameters);

        let tx = Transaction(tx, id);
        Ok(tx)
    }
}

#[derive(Default)]
pub struct TxStatusSubscription;

struct StreamState<'a> {
    txpool: &'a TxPool,
    db: &'a Database,
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
    async fn status_change<'a>(
        &self,
        ctx: &Context<'a>,
        #[graphql(desc = "The ID of the transaction")] id: TransactionId,
    ) -> impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a {
        let txpool = ctx.data_unchecked::<TxPool>();
        let db = ctx.data_unchecked::<Database>();
        let rx = txpool.tx_update_subscribe();
        let state = StreamState { txpool, db };

        transaction_status_change(state, rx, id.into())
            .await
            .map_err(async_graphql::Error::from)
    }
}

#[async_trait::async_trait]
impl<'a> TxnStatusChangeState for StreamState<'a> {
    async fn get_tx_status(
        &self,
        id: fuel_types::Bytes32,
    ) -> StorageResult<Option<TransactionStatus>> {
        types::get_tx_status(id, self.db, self.txpool).await
    }
}
