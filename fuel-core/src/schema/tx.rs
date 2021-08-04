use crate::database::DatabaseTransaction;
use crate::service::SharedDatabase;
use actix_web::web;
use async_graphql::types::EmptySubscription;
use async_graphql::{Context, Object, Schema};
use async_graphql_actix_web::{Request, Response};
use fuel_vm::prelude::*;
use tokio::task;

pub struct MutationRoot;
pub struct QueryRoot;

pub type TXSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

#[Object]
impl QueryRoot {
    async fn version(&self, _ctx: &Context<'_>) -> async_graphql::Result<String> {
        const VERSION: &str = env!("CARGO_PKG_VERSION");

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
                vm.transact(tx).map_err(Box::new)?;
                transaction.commit().map_err(Box::new)?;
                Ok(vm)
            },
        )
        .await??;

        Ok(serde_json::to_string(vm.log())?)
    }
}

// TODO: https://github.com/FuelLabs/fuel-core/issues/29
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
