use crate::serialization::HexIfHumanReadable;
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
use serde_with::serde_as;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Default)]
pub struct ContractConfig {
    pub contract_id: ContractId,
    #[serde_as(as = "HexIfHumanReadable")]
    pub code: Vec<u8>,
    pub salt: Salt,
    pub state: Option<Vec<(Bytes32, Bytes32)>>,
    pub balances: Option<Vec<(AssetId, u64)>>,
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

#[cfg(all(test, feature = "random"))]
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
            // not populated since they have to be removed from the ContractConfig
            balances: None,
            state: None,
        }
    }
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
