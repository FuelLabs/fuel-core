use crate::database::{transactional::DatabaseTransaction, KvStore, SharedDatabase};
use crate::schema::scalars::HexString256;
use crate::tx_pool::TxPool;
use async_graphql::connection::{query, Connection, Edge, EmptyFields};
use async_graphql::{Context, Object};
use fuel_tx::{Bytes32, Transaction as FuelTx};
use fuel_vm::prelude::Interpreter;
use itertools::Itertools;
use std::sync::Arc;
use tokio::task;
use types::Transaction;

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
        let db = ctx.data_unchecked::<SharedDatabase>().as_ref();
        let key = id.0.into();
        Ok(KvStore::<Bytes32, FuelTx>::get(db, &key)?.map(|tx| Transaction(tx)))
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> async_graphql::Result<Connection<usize, Transaction, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<SharedDatabase>().as_ref();
        let txs: Vec<fuel_tx::Transaction> = db.all_transactions().try_collect()?;

        query(
            after,
            before,
            first,
            last,
            |after, before, first, last| async move {
                let mut start = 0usize;
                let mut end = txs.len();

                if let Some(after) = after {
                    if after >= txs.len() {
                        return Ok(Connection::new(false, false));
                    }
                    start = after + 1;
                }

                if let Some(before) = before {
                    if before == 0 || before < start || before > txs.len() {
                        return Ok(Connection::new(false, false));
                    }
                    end = before;
                }

                let mut slice = &txs[start..end];

                if let Some(first) = first {
                    slice = &slice[..first.min(slice.len())];
                    end -= first.min(slice.len());
                } else if let Some(last) = last {
                    slice = &slice[slice.len() - last.min(slice.len())..];
                    start = end - last.min(slice.len());
                }

                let mut connection = Connection::new(start > 0, end < txs.len());
                connection.append(
                    slice
                        .iter()
                        .enumerate()
                        .map(|(idx, item)| Edge::new(start + idx, item.clone())),
                );
                Ok(connection)
            },
        )
        .await
        .map(|conn| conn.map_node(Transaction))
    }
}

#[derive(Default)]
pub struct TxMutation;

#[Object]
impl TxMutation {
    /// blocks on transaction submission until processed in a block
    async fn run(&self, ctx: &Context<'_>, tx: String) -> async_graphql::Result<String> {
        let transaction = ctx.data_unchecked::<SharedDatabase>().0.transaction();

        let vm = task::spawn_blocking(
            move || -> async_graphql::Result<Interpreter<DatabaseTransaction>> {
                let tx: FuelTx = serde_json::from_str(tx.as_str())?;
                let mut vm = Interpreter::with_storage(transaction.clone());
                vm.transact(tx).map_err(Box::new)?;
                transaction.commit().map_err(Box::new)?;
                Ok(vm)
            },
        )
        .await??;

        Ok(serde_json::to_string(vm.receipts())?)
    }

    /// Submits transaction to the pool
    async fn submit(&self, ctx: &Context<'_>, tx: String) -> async_graphql::Result<String> {
        let tx_pool = ctx.data::<Arc<TxPool>>().unwrap();
        let tx: FuelTx = serde_json::from_str(tx.as_str())?;
        let id = tx.id().clone();
        tx_pool.submit_tx(tx).await?;

        Ok(hex::encode(id))
    }
}
