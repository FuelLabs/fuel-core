use crate::serialization::{
    HexNumber,
    HexType,
};
use fuel_core_types::{
    fuel_tx::{
        Contract,
        ContractId,
        StorageSlot,
    },
    fuel_types::{
        AssetId,
        BlockHeight,
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
    pub contract_id: ContractId,
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

impl ContractConfig {
    pub fn calculate_contract_id(&mut self) {
        let bytes = &self.code;
        let salt = self.salt;
        let slots = self.state.clone().map(|slots| {
            slots
                .into_iter()
                .map(|(key, value)| StorageSlot::new(key, value))
                .collect::<Vec<_>>()
        });
        let state_root = slots
            .as_ref()
            .map(|slots| Contract::initial_state_root(slots.iter()))
            .unwrap_or(Contract::default_state_root());
        let contract = Contract::from(bytes.clone());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &state_root);
        self.contract_id = contract_id;
    }
}
