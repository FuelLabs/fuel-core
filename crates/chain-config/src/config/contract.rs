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
    pub salt: Salt,
    pub tx_id: Option<Bytes32>,
    pub output_index: Option<u8>,
    /// TxPointer: auto-generated if None
    /// used if contract is forked from another chain to preserve id & tx_pointer
    /// The block height that the contract was last used in
    pub tx_pointer_block_height: Option<BlockHeight>,
    /// TxPointer: auto-generated if None
    /// used if contract is forked from another chain to preserve id & tx_pointer
    /// The index of the originating tx within `tx_pointer_block_height`
    pub tx_pointer_tx_idx: Option<u16>,
}

impl ContractConfig {
    // TODO: Remove https://github.com/FuelLabs/fuel-core/issues/1668
    pub fn utxo_id(&self) -> Option<UtxoId> {
        match (self.tx_id, self.output_index) {
            (Some(tx_id), Some(output_index)) => Some(UtxoId::new(tx_id, output_index)),
            _ => None,
        }
    }

    // TODO: Remove https://github.com/FuelLabs/fuel-core/issues/1668
    pub fn tx_pointer(&self) -> TxPointer {
        match (self.tx_pointer_block_height, self.tx_pointer_tx_idx) {
            (Some(block_height), Some(tx_idx)) => TxPointer::new(block_height, tx_idx),
            _ => TxPointer::default(),
        }
    }
}

#[cfg(all(test, feature = "random", feature = "std"))]
impl crate::Randomize for ContractConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            contract_id: ContractId::new(super::random_bytes_32(&mut rng)),
            code: (super::random_bytes_32(&mut rng)).to_vec(),
            salt: Salt::new(super::random_bytes_32(&mut rng)),
            tx_id: rng
                .gen::<bool>()
                .then(|| super::random_bytes_32(&mut rng).into()),
            output_index: rng.gen::<bool>().then(|| rng.gen()),
            tx_pointer_block_height: rng
                .gen::<bool>()
                .then(|| BlockHeight::from(rng.gen::<u32>())),
            tx_pointer_tx_idx: rng.gen::<bool>().then(|| rng.gen()),
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
