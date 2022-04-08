use async_graphql::Enum;
pub use fuel_core_interfaces::models::{
    BlockHeight, Coin, CoinStatus, FuelBlock, FuelBlockDb, FuelBlockHeader,
};

pub type Hash = [u8; 32];

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(remote = "CoinStatus")]
pub enum SchemaCoinStatus {
    Unspent,
    Spent,
}
