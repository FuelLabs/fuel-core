use crate::database::{transactional::DatabaseTransaction, SharedDatabase};

use crate::tx_pool::TxPool;
use async_graphql::{Context, Object, SchemaBuilder};
use fuel_vm::prelude::*;
use std::sync::Arc;
use tokio::task;

#[derive(Default)]
pub struct TxQuery;

#[derive(Default)]
pub struct TxMutation;

#[Object]
impl TxQuery {
    async fn version(&self, _ctx: &Context<'_>) -> async_graphql::Result<String> {
        const VERSION: &str = env!("CARGO_PKG_VERSION");

        Ok(VERSION.to_owned())
    }
}

#[Object]
impl TxMutation {
    /// blocks on transaction submission until processed in a block
    async fn run(&self, ctx: &Context<'_>, tx: String) -> async_graphql::Result<String> {
        let transaction = ctx.data_unchecked::<SharedDatabase>().0.transaction();

        let vm = task::spawn_blocking(
            move || -> async_graphql::Result<Interpreter<DatabaseTransaction>> {
                let tx: Transaction = serde_json::from_str(tx.as_str())?;
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
        let tx: Transaction = serde_json::from_str(tx.as_str())?;
        let id = tx.id().clone();
        tx_pool.submit_tx(tx).await?;

        Ok(hex::encode(id))
    }
}
