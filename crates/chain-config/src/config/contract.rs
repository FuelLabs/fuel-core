use crate::serialization::{
    HexNumber,
    HexType,
};
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    fuel_types::{
        AssetId,
        Bytes32,
        Salt,
    },
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
    /// UtxoId: auto-generated if None
    #[serde_as(as = "Option<HexType>")]
    #[serde(default)]
    pub tx_id: Option<Bytes32>,
    /// UtxoId: auto-generated if None
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub output_index: Option<u8>,
    /// TxPointer: auto-generated if None
    /// used if contract is forked from another chain to preserve id & tx_pointer
    /// The block height that the contract was last used in
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub tx_pointer_block_height: Option<BlockHeight>,
    /// TxPointer: auto-generated if None
    /// used if contract is forked from another chain to preserve id & tx_pointer
    /// The index of the originating tx within `tx_pointer_block_height`
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub tx_pointer_tx_idx: Option<u16>,
}
