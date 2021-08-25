pub mod dap;
pub mod tx;

use async_graphql::{EmptySubscription, MergedObject, Schema, SchemaBuilder};

#[derive(MergedObject, Default)]
pub struct Query(dap::DapQuery, tx::TxQuery);

#[derive(MergedObject, Default)]
pub struct Mutation(dap::DapMutation, tx::TxMutation);

// Placeholder for when we need to add subscriptions
// #[derive(MergedSubscription, Default)]
// pub struct Subscription();

pub type CoreSchema = Schema<Query, Mutation, EmptySubscription>;

pub fn build_schema() -> SchemaBuilder<Query, Mutation, EmptySubscription> {
    Schema::build(
        Query::default(),
        Mutation::default(),
        EmptySubscription::default(),
    )
}
