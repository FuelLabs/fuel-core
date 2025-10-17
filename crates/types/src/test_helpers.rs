use crate::{
    blockchain::block::Block,
    fuel_merkle::binary::root_calculator::MerkleRootCalculator,
    fuel_tx::{
        ContractId,
        Create,
        Finalizable,
        Output,
        Script,
        Transaction,
        TransactionBuilder,
    },
    fuel_vm::{
        Contract,
        Salt,
    },
};
use proptest::prelude::*;
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
    let salt: Salt = rng.r#gen();
    let root = Contract::root_from_code(contract_code);
    let state_root = Contract::default_state_root();
    let contract_id = Contract::id(&salt, &root, &state_root);

    let tx = TransactionBuilder::create(contract_code.into(), salt, Default::default())
        .add_fee_input()
        .add_output(Output::contract_created(contract_id, state_root))
        .finalize();
    (tx, contract_id)
}

// pub enum Transaction {
//     Script(Script),
//     Create(Create),
//     Mint(Mint),
//     Upgrade(Upgrade),
//     Upload(Upload),
//     Blob(Blob),
// }
#[allow(unused)]
fn arb_txs() -> impl Strategy<Value = Vec<Transaction>> {
    let tx_strategy = prop_oneof![
        1 => arb_script_tx(),
    ];

    prop::collection::vec(tx_strategy, 1..2)
}

prop_compose! {
    fn arb_script_tx()(_: u32) -> Transaction {
        let script = Script::default();
        Transaction::Script(script)
    }
}

prop_compose! {
    /// Generate arbitrary blocks with a variable number of transactions
    pub fn arb_block()(_ in 1..100u32, txs in arb_txs()) -> Block {
        let mut fuel_block = Block::default();
        *fuel_block.transactions_mut() = txs;
        let count = fuel_block.transactions().len() as u16;
        fuel_block.header_mut().set_transactions_count(count);
        let transaction_tree =
            MerkleRootCalculator::new(
            );
        let root = transaction_tree.root().into();
        fuel_block.header_mut().set_transaction_root(root);
        fuel_block.header_mut().set_message_outbox_root(root);
        fuel_block
    }
}
