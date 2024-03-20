use crate::serialization::HexIfHumanReadable;
use fuel_core_types::{
    fuel_tx::{
        AssetId,
        Contract,
        ContractId,
        StorageSlot,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
        Salt,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::serde_as;

#[serde_as]
#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContractConfig {
    pub contract_id: ContractId,
    #[serde_as(as = "HexIfHumanReadable")]
    pub code: Vec<u8>,
    pub salt: Salt,
    pub tx_id: Bytes32,
    pub output_index: u8,
    /// TxPointer: auto-generated if None
    /// used if contract is forked from another chain to preserve id & tx_pointer
    /// The block height that the contract was last used in
    pub tx_pointer_block_height: BlockHeight,
    /// TxPointer: auto-generated if None
    /// used if contract is forked from another chain to preserve id & tx_pointer
    /// The index of the originating tx within `tx_pointer_block_height`
    pub tx_pointer_tx_idx: u16,
    pub states: Vec<ContractState>,
    pub balances: Vec<ContractAsset>,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContractState {
    pub key: Bytes32,
    #[serde_as(as = "HexIfHumanReadable")]
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContractAsset {
    pub asset_id: AssetId,
    pub amount: u64,
}

impl ContractConfig {
    pub fn utxo_id(&self) -> UtxoId {
        UtxoId::new(self.tx_id, self.output_index)
    }

    pub fn tx_pointer(&self) -> TxPointer {
        TxPointer::new(self.tx_pointer_block_height, self.tx_pointer_tx_idx)
    }
}

#[cfg(all(test, feature = "random", feature = "std"))]
impl crate::Randomize for ContractConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            contract_id: ContractId::new(super::random_bytes_32(&mut rng)),
            code: (super::random_bytes_32(&mut rng)).to_vec(),
            salt: Salt::new(super::random_bytes_32(&mut rng)),
            tx_id: super::random_bytes_32(&mut rng).into(),
            output_index: rng.gen(),
            tx_pointer_block_height: rng.gen(),
            tx_pointer_tx_idx: rng.gen(),
            states: vec![ContractState {
                key: super::random_bytes_32(&mut rng).into(),
                value: super::random_bytes_32(&mut rng).to_vec(),
            }],
            balances: vec![ContractAsset {
                asset_id: AssetId::new(super::random_bytes_32(&mut rng)),
                amount: rng.gen(),
            }],
        }
    }
}

impl ContractConfig {
    pub fn update_contract_id<'a>(
        &mut self,
        storage_slots: impl IntoIterator<Item = &'a StorageSlot>,
    ) {
        let state_root = Contract::initial_state_root(storage_slots.into_iter());

        let contract = Contract::from(self.code.clone());
        let root = contract.root();
        let contract_id = contract.id(&self.salt, &root, &state_root);
        self.contract_id = contract_id;
    }
}
