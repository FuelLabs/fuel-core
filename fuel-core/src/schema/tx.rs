use crate::{
    database::{
        transaction::OwnedTransactionIndexCursor,
        Database,
        KvStoreError,
    },
    model::BlockHeight,
    query::{
        transaction_status_change,
        TxnStatusChangeState,
    },
    schema::scalars::{
        Address,
        Bytes32,
        HexString,
        SortedTxCursor,
        TransactionId,
    },
    state::IterDirection,
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        query,
        Connection,
        Edge,
        EmptyFields,
    },
    Context,
    Object,
    Subscription,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsRef,
        fuel_tx::{
            Cacheable,
            Transaction as FuelTx,
            UniqueIdentifier,
        },
        fuel_types,
        fuel_vm::prelude::Deserializable,
    },
    db::{
        FuelBlocks,
        Transactions,
    },
    not_found,
    txpool::TxPoolMpsc,
};
use fuel_txpool::Service as TxPoolService;
use futures::{
    Stream,
    StreamExt,
    TryStreamExt,
};
use itertools::Itertools;
use std::{
    borrow::Cow,
    iter,
    ops::Deref,
    sync::Arc,
};
use tokio::sync::oneshot;
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

        let (response, receiver) = oneshot::channel();
        let _ = txpool
            .sender()
            .send(TxPoolMpsc::FindOne { id, response })
            .await;

        if let Ok(Some(transaction)) = receiver.await {
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

        query(
            after,
            before,
            first,
            last,
            |after: Option<SortedTxCursor>, before: Option<SortedTxCursor>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

                if (first.is_some() && before.is_some())
                    || (after.is_some() && before.is_some())
                    || (last.is_some() && after.is_some())
                {
                    return Err(anyhow!("Wrong argument combination"));
                }

                let block_id;
                let tx_id;

                if direction == IterDirection::Forward {
                    let after = after.map(|after| (after.block_height, after.tx_id));
                    block_id = after.map(|(height, _)| height);
                    tx_id = after.map(|(_, id)| id);
                } else {
                    let before = before.map(|before| (before.block_height, before.tx_id));
                    block_id = before.map(|(height, _)| height);
                    tx_id = before.map(|(_, id)| id);
                }

                let all_block_ids =
                    db.all_block_ids(block_id.map(Into::into), Some(direction));
                let mut started = None;

                if block_id.is_some() {
                    started = Some((block_id, tx_id));
                }

                let txs = all_block_ids
                    .flat_map(|block| {
                        block.map(|(block_height, block_id)| {
                            db.storage::<FuelBlocks>().get(&block_id)
                                .transpose()
                                .ok_or(not_found!(FuelBlocks))?
                                .map(|fuel_block| {
                                    let mut txs = fuel_block
                                        .into_owned()
                                        .transactions;

                                    if direction == IterDirection::Reverse {
                                        txs.reverse();
                                    }

                                    txs.into_iter().zip(iter::repeat(block_height))
                                })
                        })
                    })
                    .flatten_ok()
                    .skip_while(|h| {
                        if let (Ok((tx, _)), Some(end)) = (h, tx_id) {
                            tx != &end.into()
                        } else {
                            false
                        }
                    })
                    .skip(usize::from(tx_id.is_some()))
                    .take(records_to_fetch + 1);

                let tx_ids: Vec<(fuel_types::Bytes32, BlockHeight)> = txs.try_collect()?;

                let mut txs: Vec<(Cow<FuelTx>, &BlockHeight)> = tx_ids
                    .iter()
                    .take(records_to_fetch)
                    .map(|(tx_id, block_height)| -> Result<(Cow<FuelTx>, &BlockHeight), KvStoreError> {
                        let tx = db.storage::<Transactions>().get(tx_id)
                            .transpose()
                            .ok_or(not_found!(Transactions))?;

                        Ok((tx?, block_height))
                    })
                    .try_collect()?;

                if direction == IterDirection::Reverse {
                    txs.reverse()
                }

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch < tx_ids.len());

                connection.edges.extend(txs.into_iter().map(|(tx, block_height)| {
                    Edge::new(
                        SortedTxCursor::new(*block_height, Bytes32::from(tx.id())),
                        Transaction(tx.into_owned()),
                    )
                }));

                Ok::<Connection<SortedTxCursor, Transaction>, anyhow::Error>(connection)
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

        query(
            after,
            before,
            first,
            last,
            |after: Option<HexString>, before: Option<HexString>, first, last| {
                async move {
                    let (records_to_fetch, direction) = if let Some(first) = first {
                        (first, IterDirection::Forward)
                    } else if let Some(last) = last {
                        (last, IterDirection::Reverse)
                    } else {
                        (0, IterDirection::Forward)
                    };

                    if (first.is_some() && before.is_some())
                        || (after.is_some() && before.is_some())
                        || (last.is_some() && after.is_some())
                    {
                        return Err(anyhow!("Wrong argument combination"))
                    }

                    let after = after.map(OwnedTransactionIndexCursor::from);
                    let before = before.map(OwnedTransactionIndexCursor::from);

                    let start;
                    let end;

                    if direction == IterDirection::Forward {
                        start = after;
                        end = before;
                    } else {
                        start = before;
                        end = after;
                    }

                    let mut txs =
                        db.owned_transactions(&owner, start.as_ref(), Some(direction));
                    let mut started = None;
                    if start.is_some() {
                        // skip initial result
                        started = txs.next();
                    }

                    // take desired amount of results
                    let txs = txs
                        .take_while(|r| {
                            // take until we've reached the end
                            if let (Ok(t), Some(end)) = (r, end.as_ref()) {
                                if &t.0 == end {
                                    return false
                                }
                            }
                            true
                        })
                        .take(records_to_fetch)
                        .map(|res| {
                            res.and_then(|(cursor, tx_id)| {
                                let tx = db
                                    .storage::<Transactions>()
                                    .get(&tx_id)?
                                    .ok_or(not_found!(Transactions))?
                                    .into_owned();
                                Ok((cursor, tx))
                            })
                        });
                    let mut txs: Vec<(OwnedTransactionIndexCursor, FuelTx)> =
                        txs.try_collect()?;
                    if direction == IterDirection::Reverse {
                        txs.reverse();
                    }

                    let mut connection =
                        Connection::new(started.is_some(), records_to_fetch <= txs.len());
                    connection.edges.extend(txs.into_iter().map(|item| {
                        Edge::new(HexString::from(item.0), Transaction(item.1))
                    }));

                    Ok::<Connection<HexString, Transaction>, anyhow::Error>(connection)
                }
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
        let block_producer = ctx.data_unchecked::<Arc<fuel_block_producer::Producer>>();

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
            .sender()
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
        id: fuel_core_interfaces::common::fuel_types::Bytes32,
    ) -> anyhow::Result<Option<TransactionStatus>> {
        Ok(types::get_tx_status(id, &self.db, &self.txpool)
            .await
            .map_err(|e| anyhow::anyhow!("Database lookup failed {:?}", e))?)
    }
}
