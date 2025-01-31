use crate::{
    fuel_tx::{
        ContractId,
        Create,
        Finalizable,
        Output,
        TransactionBuilder,
    },
    fuel_vm::{
        Contract,
        Salt,
    },
};
use rand::Rng;

/// Helper function to create a contract creation transaction
/// from a given contract bytecode.
/// Example:
/// ```
/// let contract_bytecode = vec![];
/// let mut rng = rand::thread_rng();
/// let (tx, contract_id) = create_contract(&contract_bytecode, &mut rng);
/// ```
pub fn create_contract<R: Rng>(
    contract_code: &[u8],
    rng: &mut R,
) -> (Create, ContractId) {
    let salt: Salt = rng.gen();
    let contract = Contract::from(contract_code);
    let root = contract.root();
    let state_root = Contract::default_state_root();
    let contract_id = contract.id(&salt, &root, &state_root);

    let tx = TransactionBuilder::create(contract_code.into(), salt, Default::default())
        .add_fee_input()
        .add_output(Output::contract_created(contract_id, state_root))
        .finalize();
    (tx, contract_id)
}
