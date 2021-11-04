use crate::database::{transaction::OwnedTransactionIndexCursor, Database, KvStoreError};
use crate::schema::scalars::{HexString, HexString256};
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
    ) -> async_graphql::Result<Connection<HexString256, Transaction, EmptyFields, EmptyFields>>
    {
        let db = ctx.data_unchecked::<Database>();

        query(
            after,
            before,
            first,
            last,
            |after: Option<HexString256>, before: Option<HexString256>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

                let after = after.map(Bytes32::from);
                let before = before.map(Bytes32::from);

                let start;
                let end;

                if direction == IterDirection::Forward {
                    start = after;
                    end = before;
                } else {
                    start = before;
                    end = after;
                }

                let mut txs = db.all_transactions(start.as_ref(), Some(direction));
                let mut started = None;
                if start.is_some() {
                    // skip initial result
                    started = txs.next();
                }

                // take desired amount of results
                let txs = txs
                    .take_while(|r| {
                        // take until we've reached the end
                        if let (Ok(t), Some(end)) = (r, end) {
                            if t.id() == end {
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch);
                let mut txs: Vec<fuel_tx::Transaction> = txs.try_collect()?;
                if direction == IterDirection::Reverse {
                    txs.reverse();
                }

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= txs.len());
                connection.append(
                    txs.iter()
                        .map(|item| Edge::new(HexString256::from(item.id()), item.clone())),
                );
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
                                .ok_or_else(|| KvStoreError::NotFound)?
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
        let tx = FuelTx::from_bytes(&tx.0)?;
        // make virtual txpool from transactional view
        let tx_pool = TxPool::new(transaction.deref().clone());
        let receipts = tx_pool.run_tx(tx).await?;
        Ok(receipts.into_iter().map(receipt::Receipt).collect())
    }

    /// Submits transaction to the pool
    async fn submit(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<HexString256> {
        let tx_pool = ctx.data::<Arc<TxPool>>().unwrap();
        let tx = FuelTx::from_bytes(&tx.0)?;
        let id = tx_pool.submit_tx(tx).await?;

        Ok(id.into())
    }
}
