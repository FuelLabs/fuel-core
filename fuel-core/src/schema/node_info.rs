use super::scalars::U64;
use crate::service::Config;
use async_graphql::{
    Context,
    Object,
};

pub struct NodeInfo {
    utxo_validation: bool,
    vm_backtrace: bool,
    min_gas_price: U64,
    max_tx: U64,
    max_depth: U64,
    node_version: String,
}

#[Object]
impl NodeInfo {
    async fn utxo_validation(&self) -> bool {
        self.utxo_validation
    }

    async fn vm_backtrace(&self) -> bool {
        self.vm_backtrace
    }

    async fn min_gas_price(&self) -> U64 {
        self.min_gas_price
    }

    async fn max_tx(&self) -> U64 {
        self.max_tx
    }

    async fn max_depth(&self) -> U64 {
        self.max_depth
    }

    async fn node_version(&self) -> String {
        self.node_version.to_owned()
    }
}

#[derive(Default)]
pub struct NodeQuery {}

#[Object]
impl NodeQuery {
    async fn node_info(&self, ctx: &Context<'_>) -> async_graphql::Result<NodeInfo> {
        let Config {
            utxo_validation,
            vm,
            txpool,
            ..
        } = ctx.data_unchecked::<Config>();

        const VERSION: &str = env!("CARGO_PKG_VERSION");

        Ok(NodeInfo {
            utxo_validation: *utxo_validation,
            vm_backtrace: vm.backtrace,
            min_gas_price: txpool.min_gas_price.into(),
            max_tx: (txpool.max_tx as u64).into(),
            max_depth: (txpool.max_depth as u64).into(),
            node_version: VERSION.to_owned(),
        })
    }
}
