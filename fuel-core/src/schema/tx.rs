use crate::database::{transaction::OwnedTransactionIndexCursor, Database, KvStoreError};
use crate::model::fuel_block::{BlockHeight, FuelBlock};
use crate::schema::scalars::{HexString, HexString256, SortedTxCursor};
use crate::service::Config;
use crate::state::IterDirection;
use crate::tx_pool::TxPool;
use async_graphql::{
    connection::{query, Connection, Edge, EmptyFields},
    Context, Object,
};
use fuel_storage::Storage;
use fuel_tx::Transaction as FuelTx;
use fuel_types::{Address, Bytes32};
use fuel_vm::prelude::Deserializable;
use itertools::Itertools;
use std::borrow::Cow;
use std::iter;
use std::ops::Deref;
use std::sync::Arc;
use types::Transaction;

pub mod receipt;
pub mod types;

#[derive(Default)]
pub struct TxQuery;

#[Object]
impl TxQuery {
    async fn version(&self, _ctx: &Context<'_>) -> async_graphql::Result<String> {
        const VERSION: &str = env!("CARGO_PKG_VERSION");

        Ok(VERSION.to_owned())
    }

    async fn transaction(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "id of the transaction")] id: HexString256,
    ) -> async_graphql::Result<Option<Transaction>> {
        let db = ctx.data_unchecked::<Database>();
        let key = id.0.into();
        Ok(Storage::<Bytes32, FuelTx>::get(db, &key)?.map(|tx| Transaction(tx.into_owned())))
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
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
                            Storage::<Bytes32, FuelBlock>::get(db, &block_id)
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
                    .take(records_to_fetch);

                let mut txs: Vec<(Bytes32, BlockHeight)> = txs.try_collect()?;

                if direction == IterDirection::Reverse {
                    txs.reverse()
                }

                let txs: Vec<(Cow<FuelTx>, &BlockHeight)> = txs
                    .iter()
                    .map(|(tx_id, block_height)| -> Result<(Cow<FuelTx>, &BlockHeight), KvStoreError> {
                        let tx = Storage::<Bytes32, FuelTx>::get(db, tx_id)
                            .transpose()
                            .ok_or(KvStoreError::NotFound)?;

                        Ok((tx?, block_height))
                    })
                    .try_collect()?;

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= txs.len());

                connection.append(txs.into_iter().map(|(tx, block_height)| {
                    Edge::new(
                        SortedTxCursor::new(*block_height, HexString256::from(tx.id())),
                        tx.into_owned(),
                    )
                }));
                Ok(connection)
            },
        )
        .await
        .map(|conn| conn.map_node(Transaction))
    }

    async fn transactions_by_owner(
        &self,
        ctx: &Context<'_>,
        owner: HexString256,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> async_graphql::Result<Connection<HexString, Transaction, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>();
        let owner = Address::from(owner);

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
                            let tx = Storage::<Bytes32, FuelTx>::get(db, &tx_id)?
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
                connection.append(
                    txs.into_iter()
                        .map(|item| Edge::new(HexString::from(item.0), Transaction(item.1))),
                );
                Ok(connection)
            },
        )
        .await
    }
}

#[derive(Default)]
pub struct TxMutation;

#[Object]
impl TxMutation {
    /// dry-run the transaction using a fork of current state, no changes are committed.
    async fn dry_run(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<Vec<receipt::Receipt>> {
        let transaction = ctx.data_unchecked::<Database>().transaction();
        let cfg = ctx.data_unchecked::<Config>();
        let tx = FuelTx::from_bytes(&tx.0)?;
        // make virtual txpool from transactional view
        let tx_pool = TxPool::new(transaction.deref().clone());
        let receipts = tx_pool.run_tx(tx, cfg).await?;
        Ok(receipts.into_iter().map(receipt::Receipt).collect())
    }

    /// Submits transaction to the pool
    async fn submit(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<HexString256> {
        let tx_pool = ctx.data::<Arc<TxPool>>().unwrap();
        let cfg = ctx.data_unchecked::<Config>();
        let tx = FuelTx::from_bytes(&tx.0)?;
        let id = tx_pool.submit_tx(tx, cfg).await?;

        Ok(id.into())
    }
}
