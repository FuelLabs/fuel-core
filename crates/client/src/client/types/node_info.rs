use crate::client::schema;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeInfo {
    pub utxo_validation: bool,
    pub vm_backtrace: bool,
    pub max_tx: u64,
    pub max_depth: u64,
    pub node_version: String,
    pub current_pool_gas: u64,
}

// GraphQL Translation

impl From<schema::node_info::NodeInfo> for NodeInfo {
    fn from(value: schema::node_info::NodeInfo) -> Self {
        Self {
            utxo_validation: value.utxo_validation,
            vm_backtrace: value.vm_backtrace,
            max_tx: value.max_tx.into(),
            max_depth: value.max_depth.into(),
            node_version: value.node_version,
            current_pool_gas: value.current_pool_gas.into(),
        }
    }
}
