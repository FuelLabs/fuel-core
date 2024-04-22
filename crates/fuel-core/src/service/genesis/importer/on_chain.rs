use super::{
    import_task::ImportTable,
    Handler,
};
use crate::database::{
    balances::BalancesInitializer,
    database_description::on_chain::OnChain,
    state::StateInitializer,
    Database,
};
use anyhow::anyhow;
use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
        ProcessedTransactions,
    },
    transactional::StorageTransaction,
    StorageAsMut,
};
use fuel_core_types::{
    self,
    blockchain::primitives::DaBlockHeight,
    entities::{
        coins::coin::Coin,
        Message,
    },
    fuel_types::BlockHeight,
};
