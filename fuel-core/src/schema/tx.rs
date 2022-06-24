use crate::database::{transaction::OwnedTransactionIndexCursor, Database, KvStoreError};
use crate::executor::Executor;
use crate::model::{BlockHeight, FuelBlockDb};
use crate::schema::scalars::{Address, Bytes32, HexString, SortedTxCursor, TransactionId};
use crate::service::Config;
use crate::state::IterDirection;
use async_graphql::{
    connection::{query, Connection, Edge, EmptyFields},
    Context, Object,
};
use fuel_core_interfaces::common::{
    fuel_storage::Storage,
    fuel_tx::{Bytes32 as FuelBytes32, Receipt as FuelReceipt, Transaction as FuelTx},
    fuel_types,
    fuel_vm::prelude::Deserializable,
};
use fuel_core_interfaces::txpool::TxPoolMpsc;
use fuel_txpool::Service as TxPoolService;
use itertools::Itertools;
use std::borrow::Cow;
use std::iter;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::oneshot;
use types::Transaction;

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
            Ok(Some(Transaction((transaction.tx().deref()).clone())))
        } else {
            Ok(Storage::<fuel_types::Bytes32, FuelTx>::get(db, &id)?
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
    ) -> async_graphql::Result<Connection<SortedTxCursor, Transaction, EmptyFields, EmptyFields>>
    {
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
                            Storage::<fuel_types::Bytes32, FuelBlockDb>::get(db, &block_id)
                                .transpose()
                                .ok_or(KvStoreError::NotFound)?
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
                    .skip(if tx_id.is_some() { 1 } else { 0 })
                    .take(records_to_fetch + 1);

                let tx_ids: Vec<(fuel_types::Bytes32, BlockHeight)> = txs.try_collect()?;

                let mut txs: Vec<(Cow<FuelTx>, &BlockHeight)> = tx_ids
                    .iter()
                    .take(records_to_fetch)
                    .map(|(tx_id, block_height)| -> Result<(Cow<FuelTx>, &BlockHeight), KvStoreError> {
                        let tx = Storage::<fuel_types::Bytes32, FuelTx>::get(db, tx_id)
                            .transpose()
                            .ok_or(KvStoreError::NotFound)?;

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

                Ok::<Connection<SortedTxCursor, Transaction>, KvStoreError>(connection)
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
    ) -> async_graphql::Result<Connection<HexString, Transaction, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>();
        let owner = fuel_types::Address::from(owner);

        query(
            after,
            before,
            first,
            last,
            |after: Option<HexString>, before: Option<HexString>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

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

                let mut txs = db.owned_transactions(&owner, start.as_ref(), Some(direction));
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
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch)
                    .map(|res| {
                        res.and_then(|(cursor, tx_id)| {
                            let tx = Storage::<fuel_types::Bytes32, FuelTx>::get(db, &tx_id)?
                                .ok_or(KvStoreError::NotFound)?
                                .into_owned();
                            Ok((cursor, tx))
                        })
                    });
                let mut txs: Vec<(OwnedTransactionIndexCursor, FuelTx)> = txs.try_collect()?;
                if direction == IterDirection::Reverse {
                    txs.reverse();
                }

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= txs.len());
                connection.edges.extend(
                    txs.into_iter()
                        .map(|item| Edge::new(HexString::from(item.0), Transaction(item.1))),
                );

                Ok::<Connection<HexString, Transaction>, KvStoreError>(connection)
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
        let transaction = ctx.data_unchecked::<Database>().transaction();
        let mut cfg = ctx.data_unchecked::<Config>().clone();
        // override utxo_validation if set
        if let Some(utxo_validation) = utxo_validation {
            cfg.utxo_validation = utxo_validation;
        }
        let mut tx = FuelTx::from_bytes(&tx.0)?;
        tx.precompute_metadata();
        let id = tx.id();

        // make executor from transaction database view.
        let executor = Executor {
            database: transaction.deref().clone(),
            config: cfg.clone(),
        };
        executor.submit_txs(vec![Arc::new(tx)]).await?;
        // get receipts from db transaction
        let receipts = Storage::<FuelBytes32, Vec<FuelReceipt>>::get(transaction.deref(), &id)?
            .unwrap_or_default();
        Ok(receipts.iter().map(Into::into).collect())
    }

    /// Submits transaction to the txpool
    async fn submit(&self, ctx: &Context<'_>, tx: HexString) -> async_graphql::Result<Transaction> {
        let db = ctx.data_unchecked::<Database>();
        let txpool = ctx.data_unchecked::<Arc<TxPoolService>>();
        let cfg = ctx.data_unchecked::<Config>().clone();
        let mut tx = FuelTx::from_bytes(&tx.0)?;
        tx.precompute_metadata();

        let includable = if cfg.utxo_validation {
            // include transaction
            let (response, receiver) = oneshot::channel();
            let _ = txpool
                .sender()
                .send(TxPoolMpsc::Insert {
                    txs: vec![Arc::new(tx.clone())],
                    response,
                })
                .await;
            receiver.await?.get(0).unwrap().as_ref()?;

            // get includable transactions
            let (response, receiver) = oneshot::channel();
            let _ = txpool
                .sender()
                .send(TxPoolMpsc::Includable { response })
                .await;
            let txs = receiver.await?;

            // remove transaction
            let _ = txpool
                .sender()
                .send(TxPoolMpsc::Remove { ids: vec![tx.id()] })
                .await;

            txs
        } else {
            vec![Arc::new(tx.clone())]
        };

        // next part can be extracted to saparate endpoint that will trigger block building
        // just inlude call to txpool.includable

        let executor = Executor {
            database: db.clone(),
            config: cfg.clone(),
        };
        executor.submit_txs(includable).await?;

        // probably need to fetch executed tx that is now in db.
        let tx = Transaction(tx);
        Ok(tx)
    }
}
