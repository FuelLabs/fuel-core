//! Compressed versions of fuel-tx types needed for DA storage.

// TODO: remove malleabile fields

use fuel_core_types::{
    fuel_tx::{
        self,
        TxPointer,
    },
    fuel_types::{
        self,
        BlockHeight,
        Bytes32,
        Word,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::registry::{
    tables,
    Key,
};

use super::MaybeCompressed;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Transaction {
    Script(Script),
    Create(Create),
    Mint(Mint),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Script {
    script_gas_limit: Word,
    script: MaybeCompressed<tables::ScriptCode>,
    script_data: Vec<u8>,
    policies: fuel_tx::policies::Policies,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    witnesses: Vec<Key<tables::Witness>>,
    receipts_root: Bytes32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Input {
    CoinSigned {
        utxo_id: TxPointer,
        owner: Key<tables::Address>,
        amount: Word,
        asset_id: Key<tables::AssetId>,
        tx_pointer: TxPointer,
        witness_index: u8,
        maturity: BlockHeight,
    },
    CoinPredicate {
        utxo_id: TxPointer,
        owner: Key<tables::Address>,
        amount: Word,
        asset_id: Key<tables::AssetId>,
        tx_pointer: TxPointer,
        maturity: BlockHeight,
        predicate_gas_used: Word,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    },
    Contract {
        utxo_id: TxPointer,
        balance_root: Bytes32,
        state_root: Bytes32,
        tx_pointer: TxPointer,
        asset_id: Key<tables::AssetId>,
    },
    MessageCoinSigned {
        sender: Key<tables::Address>,
        recipient: Key<tables::Address>,
        amount: Word,
        nonce: fuel_types::Nonce,
        witness_index: u8,
        data: Vec<u8>,
    },
    MessageCoinPredicate {
        sender: Key<tables::Address>,
        recipient: Key<tables::Address>,
        amount: Word,
        nonce: fuel_types::Nonce,
        predicate_gas_used: Word,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    },
    MessageDataSigned {
        sender: Key<tables::Address>,
        recipient: Key<tables::Address>,
        amount: Word,
        nonce: fuel_types::Nonce,
        witness_index: u8,
        data: Vec<u8>,
    },
    MessageDataPredicate {
        sender: Key<tables::Address>,
        recipient: Key<tables::Address>,
        amount: Word,
        nonce: fuel_types::Nonce,
        data: Vec<u8>,
        predicate_gas_used: Word,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Output {
    Coin {
        to: Key<tables::Address>,
        amount: Word,
        asset_id: Key<tables::AssetId>,
    },

    Contract {
        input_index: u8,
        balance_root: Bytes32,
        state_root: Bytes32,
    },

    Change {
        to: Key<tables::Address>,
        amount: Word,
        asset_id: Key<tables::AssetId>,
    },

    Variable {
        to: Key<tables::Address>,
        amount: Word,
        asset_id: Key<tables::AssetId>,
    },

    ContractCreated {
        contract_id: TxPointer,
        state_root: Bytes32,
    },
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Create {
    bytecode_length: Word,
    bytecode_witness_index: u8,
    policies: fuel_tx::policies::Policies,
    storage_slots: Vec<fuel_tx::StorageSlot>,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    witnesses: Vec<fuel_tx::Witness>,
    salt: fuel_types::Salt,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Mint {
    tx_pointer: TxPointer,
    input_contract: InputContract,
    output_contract: OutputContract,
    mint_amount: Word,
    mint_asset_id: Key<tables::AssetId>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct InputContract {
    utxo_id: TxPointer,
    balance_root: Bytes32,
    state_root: Bytes32,
    tx_pointer: TxPointer,
    contract_id: TxPointer,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct OutputContract {
    input_index: u8,
    balance_root: Bytes32,
    state_root: Bytes32,
}
