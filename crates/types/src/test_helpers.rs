use crate::{
    blockchain::{
        block::Block,
        header::{
            GeneratedConsensusFields,
            generate_txns_root,
        },
        primitives::DaBlockHeight,
    },
    fuel_merkle::binary::root_calculator::MerkleRootCalculator,
    fuel_tx::{
        Bytes32,
        ContractId,
        Create,
        Finalizable,
        Input,
        MessageId,
        Output,
        Script,
        Transaction,
        TransactionBuilder,
        TxPointer,
        UtxoId,
        field::{
            Inputs,
            Policies as _,
            ReceiptsRoot,
            Script as _,
            ScriptData as _,
            ScriptGasLimit,
        },
        input::coin::CoinSigned,
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
use tai64::Tai64;

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
        receipts_root in any::<[u8; 32]>(),
        script_bytes in prop::collection::vec(any::<u8>(), 0..100),
        script_data in prop::collection::vec(any::<u8>(), 0..100),
        policies in arb_policies(),
        inputs in arb_inputs(),
        // outputs in arb_outputs(),
        // witnesses in arb_witnesses(),
    ) -> Transaction {
        let mut script = Script::default();
        *script.script_gas_limit_mut() = script_gas_limit;
        *script.receipts_root_mut() = receipts_root.into();
        *script.script_mut() = script_bytes;
        *script.script_data_mut() = script_data;
        *script.policies_mut() = policies;
        *script.inputs_mut() = inputs;
        // *script.outputs_mut() = outputs;
        // *script.witnesses_mut() = witnesses;

        Transaction::Script(script)
    }
}

// prop_compose! {
//     fn arb_create_tx()(
//         contract_code in prop::collection::vec(any::<u8>(), 0..100),
//     ) -> Create {
//         let mut create = Create::default();
//         *create.contract_code_mut() = contract_code.into();
//         create
//     }
// }

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
fn arb_inputs() -> impl Strategy<Value = Vec<Input>> {
    // pub enum Input {
    //     CoinSigned(CoinSigned),
    //     CoinPredicate(CoinPredicate),
    //     Contract(Contract),
    //     MessageCoinSigned(MessageCoinSigned),
    //     MessageCoinPredicate(MessageCoinPredicate),
    //     MessageDataSigned(MessageDataSigned),
    //     MessageDataPredicate(MessageDataPredicate),
    // }
    let strategy = prop_oneof![arb_coin_signed(), arb_coin_predicate(),];
    prop::collection::vec(strategy, 0..10)
}

prop_compose! {
        //     pub utxo_id: UtxoId,
        //     pub owner: Address,
        //     pub amount: Word,
        //     pub asset_id: AssetId,
        //     pub tx_pointer: TxPointer,
        //     pub witness_index: Specification::Witness,
        //     pub predicate_gas_used: Specification::PredicateGasUsed,
        //     pub predicate: Specification::Predicate,
        //     pub predicate_data: Specification::PredicateData,
        //     type Predicate = Empty<PredicateCode>;
        //     type PredicateData = Empty<Bytes>;
        //     type PredicateGasUsed = Empty<Word>;
        //     type Witness = u16;
    fn arb_coin_signed()(
        utxo_id in arb_utxo_id(),
        owner in arb_address(),
        amount in 1..1_000_000u64,
        asset_id in arb_asset_id(),
        tx_pointer in arb_tx_pointer(),
        witness_index in 0..1000u16,
    ) -> Input {
        let inner = CoinSigned {
            utxo_id,
            owner,
            amount,
            asset_id,
            tx_pointer,
            witness_index,
            predicate_gas_used: Default::default(),
            predicate: Default::default(),
            predicate_data: Default::default(),
        };
        Input::CoinSigned(inner)
    }
}

prop_compose! {
    fn arb_coin_predicate()(
        utxo_id in arb_utxo_id(),
        owner in arb_address(),
        amount in 1..1_000_000u64,
        asset_id in arb_asset_id(),
        tx_pointer in arb_tx_pointer(),
        predicate_gas_used in any::<u64>(),
        predicate in prop::collection::vec(any::<u8>(), 0..100),
        predicate_data in prop::collection::vec(any::<u8>(), 0..100),
    ) -> Input {
        let inner = crate::fuel_tx::input::coin::CoinPredicate {
            utxo_id,
            owner,
            amount,
            asset_id,
            tx_pointer,
            witness_index: Default::default(),
            predicate_gas_used,
            predicate: predicate.into(),
            predicate_data: predicate_data.into(),
        };
        Input::CoinPredicate(inner)
    }
}

prop_compose! {
    fn arb_utxo_id()(
        inner in any::<[u8; 32]>(),
        index in any::<u16>(),
    ) -> UtxoId {
        let tx_id = inner.into();
        UtxoId::new(tx_id, index)
    }
}

prop_compose! {
    fn arb_address()(inner in any::<[u8; 32]>()) -> crate::fuel_types::Address {
        crate::fuel_types::Address::new(inner)
    }
}

prop_compose! {
    fn arb_asset_id()(inner in any::<[u8; 32]>()) -> crate::fuel_types::AssetId {
        crate::fuel_types::AssetId::new(inner)
    }
}

prop_compose! {
    fn arb_tx_pointer()(
        block_height in 0..1_000_000u32,
        tx_index in 0..1_000u16,
    ) -> TxPointer {
        let block_height = block_height.into();
        TxPointer::new(block_height, tx_index)
    }
}

#[allow(unused)]
fn arb_msg_ids() -> impl Strategy<Value = Vec<MessageId>> {
    prop::collection::vec(arb_msg_id(), 0..10usize)
}

prop_compose! {
    // pub struct ConsensusHeader<Generated> {
    //     pub prev_root: Bytes32,
    //     pub height: BlockHeight,
    //     pub time: Tai64,
    //     pub generated: Generated,
    // }
    fn arb_consensus_header()(
        prev_root in any::<[u8; 32]>(),
        time in any::<u64>(),
    ) -> crate::blockchain::header::ConsensusHeader<GeneratedConsensusFields> {
        crate::blockchain::header::ConsensusHeader {
            prev_root:  prev_root.into(),
            height: BlockHeight::new(0),
            time: Tai64(time),
            generated: GeneratedConsensusFields::default(),
        }
    }
}

// message V1Header {
//   uint64 da_height = 1;
//   uint32 consensus_parameters_version = 2;
//   uint32 state_transition_bytecode_version = 3;
//   uint32 transactions_count = 4;
//   uint32 message_receipt_count = 5;
//   bytes transactions_root = 6;
//   bytes message_outbox_root = 7;
//   bytes event_inbox_root = 8;
//   bytes prev_root = 9;
//   uint32 height = 10;
//   uint64 time = 11;
//   bytes application_hash = 12;
//   optional bytes block_id = 13;
// }
prop_compose! {
    /// Generate an arbitrary block with a variable number of transactions
    pub fn arb_block()(
        txs in arb_txs(),
        da_height in any::<u64>(),
        consensus_parameter_version in any::<u32>(),
        state_transition_bytecode_version in any::<u32>(),
        msg_ids in arb_msg_ids(),
        event_root in any::<[u8; 32]>(),
        mut consensus_header in arb_consensus_header(),
    ) -> (Block, Vec<MessageId>, Bytes32) {
        // pub struct BlockV1<TransactionRepresentation = Transaction> {
        //     header: BlockHeader,
        //     transactions: Vec<TransactionRepresentation>,
        // }
        let mut fuel_block = Block::default();

        // include txs first to be included in calculations
        *fuel_block.transactions_mut() = txs;

        // Header
        // pub struct BlockHeaderV1 {
        //     pub(crate) application: ApplicationHeader<GeneratedApplicationFieldsV1>,
        //     pub(crate) consensus: ConsensusHeader<GeneratedConsensusFields>,
        //     pub(crate) metadata: Option<BlockHeaderMetadata>,
        // }

        // Application
        // pub struct ApplicationHeader<Generated> {
        //     pub da_height: DaBlockHeight,
        //     pub consensus_parameters_version: ConsensusParametersVersion,
        //     pub state_transition_bytecode_version: StateTransitionBytecodeVersion,
        //     pub generated: Generated,
        // }
        fuel_block.header_mut().set_da_height(DaBlockHeight(da_height));
        fuel_block.header_mut().set_consensus_parameters_version(consensus_parameter_version);
        fuel_block.header_mut().set_state_transition_bytecode_version(state_transition_bytecode_version);

        // pub struct GeneratedApplicationFieldsV1 {
        //     pub transactions_count: u16,
        //     pub message_receipt_count: u32,
        //     pub transactions_root: Bytes32,
        //     pub message_outbox_root: Bytes32,
        //     pub event_inbox_root: Bytes32,
        // }
        let count = fuel_block.transactions().len().try_into().expect("we shouldn't have more than u16::MAX transactions");
        let msg_root = msg_ids
            .iter()
            .fold(MerkleRootCalculator::new(), |mut tree, id| {
                tree.push(id.as_ref());
                tree
            })
            .root()
            .into();
        let tx_root = generate_txns_root(fuel_block.transactions());
        let event_root = event_root.into();
        fuel_block.header_mut().set_transactions_count(count);
        fuel_block.header_mut().set_message_receipt_count(msg_ids.len().try_into().expect("we shouldn't have more than u32::MAX messages"));
        fuel_block.header_mut().set_transaction_root(tx_root);
        fuel_block.header_mut().set_message_outbox_root(msg_root);
        fuel_block.header_mut().set_event_inbox_root(event_root);

        // Consensus
        // TODO: Include V2 Application with V2 Header
        let application_hash = fuel_block.header().application_v1().unwrap().hash();
        consensus_header.generated.application_hash = application_hash;
        fuel_block.header_mut().set_consensus_header(consensus_header);
        (fuel_block, msg_ids, event_root)
    }
}
