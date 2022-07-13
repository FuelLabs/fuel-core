pub use fuel_core_interfaces::{
    common::{
        fuel_tx::{ContractId, Transaction, TxId},
        fuel_types::Word,
    },
    txpool::P2PNetworkInterface,
};

pub type GasPrice = Word;
