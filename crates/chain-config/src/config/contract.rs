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
use rand::Rng;
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::serde_as;

#[serde_as]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ContractConfig {
    pub contract_id: ContractId,
    #[serde_as(as = "HexIfHumanReadable")]
    pub code: Vec<u8>,
    pub salt: Salt,
    #[serde(default = "random_tx_id")]
    pub tx_id: Bytes32,
    #[serde(default = "Default::default")]
    pub output_index: u8,
    /// TxPointer:
    /// used if contract is forked from another chain to preserve id & tx_pointer
    /// The block height that the contract was last used in
    pub tx_pointer_block_height: BlockHeight,
    /// The index of the originating tx within `tx_pointer_block_height`
    pub tx_pointer_tx_idx: u16,
}

#[cfg(feature = "std")]
fn random_tx_id() -> Bytes32 {
    let mut rng = ::rand::thread_rng();
    rng.gen()
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
            tx_pointer_block_height: BlockHeight::from(rng.gen::<u32>()),
            tx_pointer_tx_idx: rng.gen(),
        }
    }
}

impl ContractConfig {
    pub fn utxo_id(&self) -> UtxoId {
        UtxoId::new(self.tx_id, self.output_index)
    }

    pub fn tx_pointer(&self) -> TxPointer {
        TxPointer::new(self.tx_pointer_block_height, self.tx_pointer_tx_idx)
    }

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
