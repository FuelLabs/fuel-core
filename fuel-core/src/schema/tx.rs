use crate::database::{transactional::DatabaseTransaction, KvStore, KvStoreError, SharedDatabase};
use crate::tx_pool::TxPool;
use async_graphql::connection::{Connection, EmptyFields};
use async_graphql::{Context, Object, SimpleObject};
use fuel_tx::{Bytes32, ContractId, Input, Output, Transaction as FuelTx, Witness};
use fuel_vm::prelude::{Color, Interpreter, Word};
use std::sync::Arc;
use tokio::task;
//
// pub struct Transaction(Bytes32);
//
// #[Object]
// impl Transaction {
//     async fn id(&self) -> Bytes32 {
//         self.0
//     }
//
//     async fn input_colors(&self, ctx: &Context<'_>) -> Result<Vec<Color>, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.input_colors().collect())
//     }
//
//     async fn input_contracts(&self, ctx: &Context<'_>) -> Result<Vec<ContractId>, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.input_contracts().collect())
//     }
//
//     async fn gas_price(&self, ctx: &Context<'_>) -> Result<Word, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.gas_price())
//     }
//
//     async fn gas_limit(&self, ctx: &Context<'_>) -> Result<Word, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.gas_limit())
//     }
//
//     async fn maturity(&self, ctx: &Context<'_>) -> Result<Word, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.maturity())
//     }
//
//     async fn is_script(&self, ctx: &Context<'_>) -> Result<bool, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.is_script())
//     }
//
//     async fn inputs(&self, ctx: &Context<'_>) -> Result<Vec<Input>, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.inputs().to_vec())
//     }
//
//     async fn outputs(&self, ctx: &Context<'_>) -> Result<Vec<Output>, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.outputs().to_vec())
//     }
//
//     async fn witnesses(&self, ctx: &Context<'_>) -> Result<Vec<Witness>, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.witnesses().to_vec())
//     }
//
//     async fn receipts_root(&self, ctx: &Context<'_>) -> Result<Option<Bytes32>, KvStoreError> {
//         let tx = get_tx(ctx, self.0)?;
//         Ok(tx.receipts_root().cloned())
//     }
// }
//
// fn get_tx(ctx: &Context<'_>, id: Bytes32) -> Result<FuelTx, KvStoreError> {
//     let db = ctx.data_unchecked::<SharedDatabase>();
//     KvStore::<Bytes32, FuelTx>::get(&*db.0.as_ref().as_ref(), &id)?.ok_or(KvStoreError::NotFound)
// }

#[derive(Default)]
pub struct TxQuery;

#[Object]
impl TxQuery {
    async fn version(&self, _ctx: &Context<'_>) -> async_graphql::Result<String> {
        const VERSION: &str = env!("CARGO_PKG_VERSION");

        Ok(VERSION.to_owned())
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
