use crate::serialization::{
    HexNumber,
    HexType,
};
use fuel_core_types::fuel_types::{
    AssetId,
    Bytes32,
    Salt,
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
pub struct ContractConfig {
    #[serde_as(as = "HexType")]
    pub code: Vec<u8>,
    #[serde_as(as = "HexType")]
    pub salt: Salt,
    #[serde_as(as = "Option<Vec<(HexType, HexType)>>")]
    #[serde(default)]
    pub state: Option<Vec<(Bytes32, Bytes32)>>,
    #[serde_as(as = "Option<Vec<(HexType, HexNumber)>>")]
    #[serde(default)]
    pub balances: Option<Vec<(AssetId, u64)>>,
}
