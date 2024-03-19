use crate::serialization::HexIfHumanReadable;
use fuel_core_types::{
    fuel_tx::{
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
            tx_id: super::random_bytes_32(&mut rng).into(),
            output_index: rng.gen(),
            tx_pointer_block_height: rng.gen(),
            tx_pointer_tx_idx: rng.gen(),
        }
    }
}

impl ContractConfig {
    pub fn update_contract_id<'a>(
        &mut self,
        salt: Salt,
        storage_slots: impl IntoIterator<Item = &'a StorageSlot>,
    ) {
        let state_root = Contract::initial_state_root(storage_slots.into_iter());

        let contract = Contract::from(self.code.clone());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &state_root);
        self.contract_id = contract_id;
    }
}
