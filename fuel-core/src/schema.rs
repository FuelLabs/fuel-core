use async_graphql::{EmptySubscription, MergedObject, Schema, SchemaBuilder};

pub mod balance;
pub mod block;
pub mod chain;
pub mod coin;
pub mod contract;
pub mod dap;
pub mod health;
pub mod message;
pub mod node_info;
pub mod scalars;
pub mod tx;

#[derive(MergedObject, Default)]
pub struct Query(
    dap::DapQuery,
    balance::BalanceQuery,
    block::BlockQuery,
    chain::ChainQuery,
    tx::TxQuery,
    health::HealthQuery,
    coin::CoinQuery,
    contract::ContractQuery,
    contract::ContractBalanceQuery,
    node_info::NodeQuery,
    message::MessageQuery,
);

#[derive(MergedObject, Default)]
pub struct Mutation(dap::DapMutation, tx::TxMutation, block::BlockMutation);

// Placeholder for when we need to add subscriptions
// #[derive(MergedSubscription, Default)]
// pub struct Subscription();

pub type CoreSchema = Schema<Query, Mutation, EmptySubscription>;

pub fn build_schema() -> SchemaBuilder<Query, Mutation, EmptySubscription> {
    Schema::build_with_ignore_name_conflicts(
        Query::default(),
        Mutation::default(),
        EmptySubscription::default(),
        ["TransactionConnection", "DaMessageConnection"],
    )
}
