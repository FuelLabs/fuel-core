use crate::serialization::{
    HexNumber,
    HexType,
};
use fuel_core_interfaces::{
    common::fuel_types::{
        Address,
        AssetId,
        Bytes32,
    },
    model::BlockHeight,
};

use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
};

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct CoinConfig {
    /// auto-generated if None
    #[serde_as(as = "Option<HexType>")]
    #[serde(default)]
    pub tx_id: Option<Bytes32>,
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub output_index: Option<u64>,
    /// used if coin is forked from another chain to preserve id
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub block_created: Option<BlockHeight>,
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub maturity: Option<BlockHeight>,
    #[serde_as(as = "HexType")]
    pub owner: Address,
    #[serde_as(as = "HexNumber")]
    pub amount: u64,
    #[serde_as(as = "HexType")]
    pub asset_id: AssetId,
}
