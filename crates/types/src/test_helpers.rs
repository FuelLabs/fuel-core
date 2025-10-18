use crate::{
    blockchain::{
        block::Block,
        header::generate_txns_root,
    },
    fuel_merkle::binary::root_calculator::MerkleRootCalculator,
    fuel_tx::{
        ContractId,
        Create,
        Finalizable,
        MessageId,
        Output,
        Script,
        Transaction,
        TransactionBuilder,
        field::{
            Policies as _,
            ReceiptsRoot,
            Script as _,
            ScriptData as _,
            ScriptGasLimit,
        },
        policies::Policies,
    },
    fuel_types::BlockHeight,
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

//     pub(crate) body: Body,
//     pub(crate) policies: Policies,
//     pub(crate) inputs: Vec<Input>,
//     pub(crate) outputs: Vec<Output>,
//     pub(crate) witnesses: Vec<Witness>,
//     pub(crate) metadata: Option<ChargeableMetadata<MetadataBody>>,
// body
//     pub(crate) script_gas_limit: Word,
//     pub(crate) receipts_root: Bytes32,
//     pub(crate) script: ScriptCode,
//     pub(crate) script_data: Bytes,
prop_compose! {
    fn arb_script_tx()(
        script_gas_limit in 1..10000u64,
        recipts_root in any::<[u8; 32]>(),
        script_bytes in prop::collection::vec(any::<u8>(), 0..100),
        script_data in prop::collection::vec(any::<u8>(), 0..100),
        policies in arb_policies(),
        // inputs in arb_inputs(),
        // outputs in arb_outputs(),
        // witnesses in arb_witnesses(),
    ) -> Transaction {
        let mut script = Script::default();
        *script.script_gas_limit_mut() = script_gas_limit;
        *script.receipts_root_mut() = recipts_root.into();
        *script.script_mut() = script_bytes;
        *script.script_data_mut() = script_data.into();
        *script.policies_mut() = policies;
        // *script.inputs_mut() = inputs;
        // *script.outputs_mut() = outputs;
        // *script.witnesses_mut() = witnesses;

        Transaction::Script(script)
    }
}

prop_compose! {
    fn arb_policies()(
        maturity in prop::option::of(0..100u32),
    ) -> Policies {
        let mut policies = Policies::new();
        if let Some(inner) = maturity {
            policies = policies.with_maturity(BlockHeight::new(inner));
        }
        policies
    }
}

prop_compose! {
    fn arb_msg_id()(inner in any::<[u8; 32]>()) -> MessageId {
        MessageId::new(inner)
    }
}

#[allow(unused)]
fn arb_msg_ids() -> impl Strategy<Value = Vec<MessageId>> {
    prop::collection::vec(arb_msg_id(), 0..10usize)
}

prop_compose! {
    /// Generate an arbitrary block with a variable number of transactions
    pub fn arb_block()(
        txs in arb_txs(),
        msg_ids in arb_msg_ids(),
    ) -> (Block, Vec<MessageId>) {
        let mut fuel_block = Block::default();
        *fuel_block.transactions_mut() = txs;
        let count = fuel_block.transactions().len() as u16;
        fuel_block.header_mut().set_transactions_count(count);
        let tx_root = generate_txns_root(fuel_block.transactions());
        fuel_block.header_mut().set_transaction_root(tx_root);
        let msg_root = msg_ids
            .iter()
            .fold(MerkleRootCalculator::new(), |mut tree, id| {
                tree.push(id.as_ref());
                tree
            })
            .root()
            .into();
        fuel_block.header_mut().set_message_outbox_root(msg_root);
        fuel_block.header_mut().set_message_receipt_count(msg_ids.len() as u32);
        fuel_block.header_mut().recalculate_metadata();
        (fuel_block, msg_ids)
    }
}
