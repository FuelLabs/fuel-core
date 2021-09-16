use crate::database::{transactional::DatabaseTransaction, KvStore, SharedDatabase};
use crate::schema::scalars::HexString256;
use crate::tx_pool::TxPool;
use async_graphql::connection::{Connection, EmptyFields};
use async_graphql::{Context, Object};
use fuel_tx::{Bytes32, Transaction as FuelTx};
use fuel_vm::prelude::Interpreter;
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

    //
    // async fn transactions(
    //     &self,
    //     ctx: &Context<'_>,
    // ) -> async_graphql::Result<Connection<Bytes32, Transaction, EmptyFields, EmptyFields>> {
    //     Ok(TransactionResponse)
    // }
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
