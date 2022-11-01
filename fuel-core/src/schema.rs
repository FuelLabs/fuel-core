use async_graphql::{
    MergedObject,
    MergedSubscription,
    Schema,
    SchemaBuilder,
};

pub mod balance;
pub mod block;
pub mod chain;
pub mod coin;
pub mod contract;
pub mod dap;
pub mod health;
pub mod message;
pub mod node_info;
pub mod resource;
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
    resource::ResourceQuery,
);

#[derive(MergedObject, Default)]
pub struct Mutation(dap::DapMutation, tx::TxMutation, block::BlockMutation);

#[derive(MergedSubscription, Default)]
pub struct Subscription(tx::TxStatusSubscription);

pub type CoreSchema = Schema<Query, Mutation, Subscription>;

pub fn build_schema() -> SchemaBuilder<Query, Mutation, Subscription> {
    Schema::build_with_ignore_name_conflicts(
        Query::default(),
        Mutation::default(),
        Subscription::default(),
        ["TransactionConnection", "MessageConnection"],
    )
}
