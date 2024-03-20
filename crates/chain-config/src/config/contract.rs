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
use itertools::Itertools;
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

impl TryFrom<ContractState> for StorageSlot {
    type Error = anyhow::Error;

    fn try_from(value: ContractState) -> Result<Self, Self::Error> {
        let key = value.key;
        let value = Bytes32::try_from(value.value.as_slice())?;
        Ok(Self::new(key, value))
    }
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for ContractState {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            key: crate::Randomize::randomize(&mut rng),
            value: Bytes32::randomize(&mut rng).to_vec(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContractAsset {
    pub asset_id: AssetId,
    pub amount: u64,
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for ContractAsset {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            asset_id: crate::Randomize::randomize(&mut rng),
            amount: crate::Randomize::randomize(&mut rng),
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
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for ContractConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        let code = Bytes32::randomize(&mut rng).to_vec();
        let salt = crate::Randomize::randomize(&mut rng);
        let states = vec![ContractState {
            key: Bytes32::randomize(&mut rng),
            value: Bytes32::randomize(&mut rng).to_vec(),
        }];

        let contract = Contract::from(code.clone());
        let state_root = {
            let states = states
                .iter()
                .cloned()
                .map(|state| StorageSlot::try_from(state).expect("32 bytes"))
                .collect_vec();
            Contract::initial_state_root(states.iter())
        };
        let contract_id = contract.id(&salt, &contract.root(), &state_root);

        Self {
            contract_id,
            code,
            salt,
            tx_id: crate::Randomize::randomize(&mut rng),
            output_index: rng.gen(),
            tx_pointer_block_height: crate::Randomize::randomize(&mut rng),
            tx_pointer_tx_idx: rng.gen(),
            states,
            balances: vec![ContractAsset {
                asset_id: crate::Randomize::randomize(&mut rng),
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
