use crate::database::{Database, DatabaseTransaction};
use crate::service::SharedDatabase;
use actix_web::web;
use async_graphql::types::EmptySubscription;
use async_graphql::{Context, Object, Schema};
use async_graphql_actix_web::{Request, Response};
use fuel_vm::prelude::*;
use futures::lock::Mutex;
use std::sync;
use tokio::task;

pub type TxStorage = sync::Arc<Mutex<Database>>;
pub struct MutationRoot;
pub struct QueryRoot;

pub type TXSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

#[Object]
impl QueryRoot {
    async fn version(&self, _ctx: &Context<'_>) -> async_graphql::Result<String> {
        const VERSION: &'static str = env!("CARGO_PKG_VERSION");

        Ok(VERSION.to_owned())
    }
}

#[Object]
impl MutationRoot {
    async fn run(&self, ctx: &Context<'_>, tx: String) -> async_graphql::Result<String> {
        let transaction = ctx.data_unchecked::<SharedDatabase>().0.transaction();

        let vm = task::spawn_blocking(
            move || -> async_graphql::Result<Interpreter<DatabaseTransaction>> {
                let tx: Transaction = serde_json::from_str(tx.as_str())?;
                let mut vm = Interpreter::with_storage(transaction.clone());
                vm.init(tx).map_err(|e| Box::new(e))?;
                vm.run().map_err(|e| Box::new(e))?;

                transaction.commit().map_err(|e| Box::new(e))?;
                Ok(vm)
            },
        )
        .await??;

        Ok(serde_json::to_string(vm.log())?)
    }
}

pub fn schema(state: Option<SharedDatabase>) -> TXSchema {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription);
    if let Some(database) = state {
        schema.data(database)
    } else {
        schema
    }
    .finish()
}

pub async fn service(schema: web::Data<TXSchema>, req: Request) -> Response {
    schema.execute(req.into_inner()).await.into()
}
