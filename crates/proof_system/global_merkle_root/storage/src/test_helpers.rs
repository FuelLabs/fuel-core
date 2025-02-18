// Hack to circumvent `unused_crate_dependencies` lint
// while enabling the "test-helpers" feature as "dev-dependency".
use crate as fuel_core_global_merkle_root_storage;
use fuel_core_global_merkle_root_storage::Dummy;
/// Dummy type
pub const DUMMY: Dummy = Dummy;

use fuel_core_types::{
    fuel_tx::{
        Address,
        Bytes32,
        ContractId,
        Create,
        Finalizable as _,
        Output,
        TransactionBuilder,
        TxId,
        TxPointer,
        UtxoId,
    },
    fuel_types::BlockHeight,
    fuel_vm::{
        Contract,
        Salt,
    },
};

use rand::Rng;

// TODO: https://github.com/FuelLabs/fuel-core/issues/2654
// This code is copied from the executor. We should refactor it to be shared.
/// Create a contract create transaction
pub fn create_contract_tx(bytecode: &[u8], rng: &mut impl rand::RngCore) -> Create {
    let salt: Salt = rng.gen();
    let contract = Contract::from(bytecode);
    let root = contract.root();
    let state_root = Contract::default_state_root();
    let contract_id = contract.id(&salt, &root, &state_root);

    TransactionBuilder::create(bytecode.into(), salt, Default::default())
        .add_fee_input()
        .add_output(Output::contract_created(contract_id, state_root))
        .finalize()
}

/// Sample a random UTxO ID
pub fn random_utxo_id(rng: &mut impl rand::RngCore) -> UtxoId {
    let mut txid = TxId::default();
    rng.fill_bytes(txid.as_mut());
    let output_index = rng.gen();

    UtxoId::new(txid, output_index)
}

/// Sample a random transaction pointer
pub fn random_tx_pointer(rng: &mut impl rand::RngCore) -> TxPointer {
    let block_height = BlockHeight::new(rng.gen());
    let tx_index = rng.gen();

    TxPointer::new(block_height, tx_index)
}

/// Sample a random address
pub fn random_address(rng: &mut impl rand::RngCore) -> Address {
    let mut address = Address::default();
    rng.fill_bytes(address.as_mut());

    address
}

/// Sample a random contract ID
pub fn random_contract_id(rng: &mut impl rand::RngCore) -> ContractId {
    let mut contract_id = ContractId::default();
    rng.fill_bytes(contract_id.as_mut());

    contract_id
}

/// Sample some random bytes
pub fn random_bytes(rng: &mut impl rand::RngCore) -> Bytes32 {
    let mut bytes = Bytes32::default();
    rng.fill_bytes(bytes.as_mut());

    bytes
}
