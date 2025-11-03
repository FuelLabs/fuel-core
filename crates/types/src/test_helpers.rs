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
        BlobBody,
        BlobIdExt,
        Bytes32,
        ContractId,
        Create,
        Finalizable,
        Input,
        MessageId,
        Output,
        StorageSlot,
        Transaction,
        TransactionBuilder,
        TxPointer,
        UpgradePurpose,
        UploadBody,
        UtxoId,
        Witness,
        field::ReceiptsRoot,
        input::{
            coin::CoinSigned,
            contract::Contract as InputContract,
        },
        output::contract::Contract as OutputContract,
        policies::Policies,
    },
    fuel_types::{
        Address,
        AssetId,
        BlobId,
        BlockHeight,
        Nonce,
    },
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

#[allow(unused)]
fn arb_txs() -> impl Strategy<Value = Vec<Transaction>> {
    prop::collection::vec(arb_transaction(), 0..10)
}

fn arb_script_transaction() -> impl Strategy<Value = Transaction> {
    (
        1..10000u64,
        any::<[u8; 32]>(),
        prop::collection::vec(any::<u8>(), 0..100),
        prop::collection::vec(any::<u8>(), 0..100),
        arb_policies(),
        arb_inputs(),
        arb_outputs(),
        prop::collection::vec(arb_witness(), 0..4),
    )
        .prop_map(
            |(
                script_gas_limit,
                receipts_root,
                script_bytes,
                script_data,
                policies,
                inputs,
                outputs,
                witnesses,
            )| {
                let mut script = crate::fuel_tx::Transaction::script(
                    script_gas_limit,
                    script_bytes,
                    script_data,
                    policies,
                    inputs,
                    outputs,
                    witnesses,
                );
                *script.receipts_root_mut() = receipts_root.into();
                Transaction::Script(script)
            },
        )
}

prop_compose! {
    fn arb_storage_slot()(
        key in any::<[u8; 32]>(),
        value in any::<[u8; 32]>(),
    ) -> StorageSlot {
        StorageSlot::new(key.into(), value.into())
    }
}

prop_compose! {
    fn arb_coin_output()(
        to in arb_address(),
        amount in any::<u64>(),
        asset in arb_asset_id(),
    ) -> Output {
        Output::coin(to, amount, asset)
    }
}

prop_compose! {
    fn arb_contract_output()(
        input_index in any::<u16>(),
        balance_root in any::<[u8; 32]>(),
        state_root in any::<[u8; 32]>(),
    ) -> Output {
        Output::Contract(OutputContract {
            input_index,
            balance_root: balance_root.into(),
            state_root: state_root.into(),
        })
    }
}

prop_compose! {
    fn arb_change_output()(
        to in arb_address(),
        amount in any::<u64>(),
        asset in arb_asset_id(),
    ) -> Output {
        Output::change(to, amount, asset)
    }
}

prop_compose! {
    fn arb_variable_output()(
        to in arb_address(),
        amount in any::<u64>(),
        asset in arb_asset_id(),
    ) -> Output {
        Output::variable(to, amount, asset)
    }
}

prop_compose! {
    fn arb_contract_created_output()(
        contract_id in any::<[u8; 32]>(),
        state_root in any::<[u8; 32]>(),
    ) -> Output {
        Output::contract_created(ContractId::new(contract_id), state_root.into())
    }
}

fn arb_output_any() -> impl Strategy<Value = Output> {
    prop_oneof![
        arb_coin_output(),
        arb_contract_output(),
        arb_change_output(),
        arb_variable_output(),
        arb_contract_created_output(),
    ]
}

fn arb_outputs() -> impl Strategy<Value = Vec<Output>> {
    prop::collection::vec(arb_output_any(), 0..10)
}

fn arb_witness() -> impl Strategy<Value = Witness> {
    prop::collection::vec(any::<u8>(), 0..128).prop_map(Witness::from)
}

prop_compose! {
    fn arb_policies()(
        tip in prop::option::of(any::<u64>()),
        witness_limit in prop::option::of(any::<u64>()),
        maturity in prop::option::of(0..100u32),
        max_fee in prop::option::of(any::<u64>()),
        expiration in prop::option::of(0..100u32),
        owner in prop::option::of(any::<u64>()),
    ) -> Policies {
        let mut policies = Policies::new();
        if let Some(tip) = tip {
            policies = policies.with_tip(tip);
        }
        if let Some(witness_limit) = witness_limit {
            policies = policies.with_witness_limit(witness_limit);
        }
        if let Some(value) = maturity {
            policies = policies.with_maturity(BlockHeight::new(value));
        }
        if let Some(max_fee) = max_fee {
            policies = policies.with_max_fee(max_fee);
        }
        if let Some(value) = expiration {
            policies = policies.with_expiration(BlockHeight::new(value));
        }
        if let Some(owner) = owner {
            policies = policies.with_owner(owner);
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
    prop::collection::vec(arb_input_any(), 0..10)
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
    fn arb_contract_input_variant()(
        utxo_id in arb_utxo_id(),
        balance_root in any::<[u8; 32]>(),
        state_root in any::<[u8; 32]>(),
        tx_pointer in arb_tx_pointer(),
        contract_id in any::<[u8; 32]>(),
    ) -> Input {
        let contract = InputContract {
            utxo_id,
            balance_root: balance_root.into(),
            state_root: state_root.into(),
            tx_pointer,
            contract_id: ContractId::new(contract_id),
        };
        Input::Contract(contract)
    }
}

prop_compose! {
    fn arb_nonce()(bytes in any::<[u8; 32]>()) -> Nonce {
        Nonce::new(bytes)
    }
}

prop_compose! {
    fn arb_message_coin_signed_input()(
        sender in arb_address(),
        recipient in arb_address(),
        amount in any::<u64>(),
        nonce in arb_nonce(),
        witness_index in any::<u16>(),
    ) -> Input {
        Input::message_coin_signed(sender, recipient, amount, nonce, witness_index)
    }
}

prop_compose! {
    fn arb_message_coin_predicate_input()(
        sender in arb_address(),
        recipient in arb_address(),
        amount in any::<u64>(),
        nonce in arb_nonce(),
        predicate_gas_used in any::<u64>(),
        predicate in prop::collection::vec(any::<u8>(), 0..64),
        predicate_data in prop::collection::vec(any::<u8>(), 0..64),
    ) -> Input {
        Input::message_coin_predicate(
            sender,
            recipient,
            amount,
            nonce,
            predicate_gas_used,
            predicate,
            predicate_data,
        )
    }
}

prop_compose! {
    fn arb_message_data_signed_input()(
        sender in arb_address(),
        recipient in arb_address(),
        amount in any::<u64>(),
        nonce in arb_nonce(),
        witness_index in any::<u16>(),
        data in prop::collection::vec(any::<u8>(), 0..128),
    ) -> Input {
        Input::message_data_signed(sender, recipient, amount, nonce, witness_index, data)
    }
}

prop_compose! {
    fn arb_message_data_predicate_input()(
        sender in arb_address(),
        recipient in arb_address(),
        amount in any::<u64>(),
        nonce in arb_nonce(),
        predicate_gas_used in any::<u64>(),
        data in prop::collection::vec(any::<u8>(), 0..128),
        predicate in prop::collection::vec(any::<u8>(), 0..64),
        predicate_data in prop::collection::vec(any::<u8>(), 0..64),
    ) -> Input {
        Input::message_data_predicate(
            sender,
            recipient,
            amount,
            nonce,
            predicate_gas_used,
            data,
            predicate,
            predicate_data,
        )
    }
}

fn arb_input_any() -> impl Strategy<Value = Input> {
    prop_oneof![
        arb_coin_signed(),
        arb_coin_predicate(),
        arb_contract_input_variant(),
        arb_message_coin_signed_input(),
        arb_message_coin_predicate_input(),
        arb_message_data_signed_input(),
        arb_message_data_predicate_input(),
    ]
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

fn arb_transaction() -> impl Strategy<Value = Transaction> {
    prop_oneof![
        arb_script_transaction(),
        arb_create_transaction(),
        arb_mint_transaction(),
        arb_upgrade_transaction(),
        arb_upload_transaction(),
        arb_blob_transaction(),
    ]
}

prop_compose! {
    fn arb_input_contract_core()(
        utxo_id in arb_utxo_id(),
        balance_root in any::<[u8; 32]>(),
        state_root in any::<[u8; 32]>(),
        tx_pointer in arb_tx_pointer(),
        contract_id in any::<[u8; 32]>(),
    ) -> InputContract {
        InputContract {
            utxo_id,
            balance_root: balance_root.into(),
            state_root: state_root.into(),
            tx_pointer,
            contract_id: ContractId::new(contract_id),
        }
    }
}

prop_compose! {
    fn arb_output_contract_core()(
        input_index in any::<u16>(),
        balance_root in any::<[u8; 32]>(),
        state_root in any::<[u8; 32]>(),
    ) -> OutputContract {
        OutputContract {
            input_index,
            balance_root: balance_root.into(),
            state_root: state_root.into(),
        }
    }
}

fn arb_create_transaction() -> impl Strategy<Value = Transaction> {
    (
        arb_policies(),
        any::<[u8; 32]>(),
        prop::collection::vec(arb_storage_slot(), 0..4),
        arb_inputs(),
        arb_outputs(),
        prop::collection::vec(arb_witness(), 1..4),
    )
        .prop_map(
            |(policies, salt_bytes, storage_slots, inputs, outputs, witnesses)| {
                let create = crate::fuel_tx::Transaction::create(
                    0,
                    policies,
                    Salt::from(salt_bytes),
                    storage_slots,
                    inputs,
                    outputs,
                    witnesses,
                );
                Transaction::Create(create)
            },
        )
}

fn arb_mint_transaction() -> impl Strategy<Value = Transaction> {
    (
        arb_tx_pointer(),
        arb_input_contract_core(),
        arb_output_contract_core(),
        any::<u64>(),
        arb_asset_id(),
        any::<u64>(),
    )
        .prop_map(
            |(
                tx_pointer,
                input_contract,
                output_contract,
                mint_amount,
                mint_asset_id,
                gas_price,
            )| {
                let mint = crate::fuel_tx::Transaction::mint(
                    tx_pointer,
                    input_contract,
                    output_contract,
                    mint_amount,
                    mint_asset_id,
                    gas_price,
                );
                Transaction::Mint(mint)
            },
        )
}

fn arb_upgrade_transaction() -> impl Strategy<Value = Transaction> {
    prop_oneof![
        (
            arb_policies(),
            arb_inputs(),
            arb_outputs(),
            prop::collection::vec(arb_witness(), 1..4),
            any::<[u8; 32]>(),
        )
            .prop_map(
                |(policies, inputs, outputs, witnesses, checksum_bytes)| {
                    let purpose = UpgradePurpose::ConsensusParameters {
                        witness_index: 0,
                        checksum: checksum_bytes.into(),
                    };
                    let upgrade = crate::fuel_tx::Transaction::upgrade(
                        purpose, policies, inputs, outputs, witnesses,
                    );
                    Transaction::Upgrade(upgrade)
                }
            ),
        (
            arb_policies(),
            arb_inputs(),
            arb_outputs(),
            prop::collection::vec(arb_witness(), 0..4),
            any::<[u8; 32]>(),
        )
            .prop_map(|(policies, inputs, outputs, witnesses, root_bytes)| {
                let purpose = UpgradePurpose::StateTransition {
                    root: root_bytes.into(),
                };
                let upgrade = crate::fuel_tx::Transaction::upgrade(
                    purpose, policies, inputs, outputs, witnesses,
                );
                Transaction::Upgrade(upgrade)
            })
    ]
}

fn arb_upload_transaction() -> impl Strategy<Value = Transaction> {
    (
        arb_policies(),
        arb_inputs(),
        arb_outputs(),
        prop::collection::vec(arb_witness(), 1..4),
        any::<[u8; 32]>(),
        prop::collection::vec(any::<[u8; 32]>(), 0..4),
        1u16..=4,
        any::<u16>(),
    )
        .prop_map(
            |(
                policies,
                inputs,
                outputs,
                witnesses,
                root_bytes,
                proof_entries,
                subsections_number,
                subsection_index_candidate,
            )| {
                let proof_set = proof_entries
                    .into_iter()
                    .map(Bytes32::from)
                    .collect::<Vec<_>>();
                let subsections_number = subsections_number.max(1);
                let body = UploadBody {
                    root: root_bytes.into(),
                    witness_index: 0,
                    subsection_index: subsection_index_candidate,
                    subsections_number,
                    proof_set,
                };
                let upload = crate::fuel_tx::Transaction::upload(
                    body, policies, inputs, outputs, witnesses,
                );
                Transaction::Upload(upload)
            },
        )
}

fn arb_blob_transaction() -> impl Strategy<Value = Transaction> {
    (
        arb_policies(),
        arb_inputs(),
        arb_outputs(),
        prop::collection::vec(arb_witness(), 0..3),
        prop::collection::vec(any::<u8>(), 0..256),
    )
        .prop_map(|(policies, inputs, outputs, witnesses, payload)| {
            let blob_id = BlobId::compute(&payload);
            let body = BlobBody {
                id: blob_id,
                witness_index: 0,
            };
            let blob = crate::fuel_tx::Transaction::blob(
                body, policies, inputs, outputs, witnesses,
            );
            Transaction::Blob(blob)
        })
}

/// Deterministic `Input::coin_signed` sample used for round-trip testing.
pub fn sample_coin_signed_input() -> Input {
    let utxo_id = UtxoId::new(Bytes32::from([1u8; 32]), 0);
    let owner = Address::new([2u8; 32]);
    let asset_id = AssetId::new([3u8; 32]);
    let tx_pointer = TxPointer::new(BlockHeight::new(0), 0);
    Input::coin_signed(utxo_id, owner, 42, asset_id, tx_pointer, 0)
}

/// Deterministic `Input::coin_predicate` sample used for round-trip testing.
pub fn sample_coin_predicate_input() -> Input {
    let utxo_id = UtxoId::new(Bytes32::from([4u8; 32]), 1);
    let owner = Address::new([5u8; 32]);
    let asset_id = AssetId::new([6u8; 32]);
    let tx_pointer = TxPointer::new(BlockHeight::new(1), 1);
    Input::coin_predicate(
        utxo_id,
        owner,
        84,
        asset_id,
        tx_pointer,
        10,
        vec![0xaa, 0xbb],
        vec![0xcc, 0xdd],
    )
}

/// Deterministic `Input::Contract` sample used for round-trip testing.
pub fn sample_contract_input() -> Input {
    let contract = InputContract {
        utxo_id: UtxoId::new(Bytes32::from([7u8; 32]), 2),
        balance_root: Bytes32::from([8u8; 32]),
        state_root: Bytes32::from([9u8; 32]),
        tx_pointer: TxPointer::new(BlockHeight::new(2), 2),
        contract_id: ContractId::new([10u8; 32]),
    };
    Input::Contract(contract)
}

/// Deterministic `Input::message_coin_signed` sample used for round-trip testing.
pub fn sample_message_coin_signed_input() -> Input {
    let sender = Address::new([11u8; 32]);
    let recipient = Address::new([12u8; 32]);
    let nonce = Nonce::new([13u8; 32]);
    Input::message_coin_signed(sender, recipient, 21, nonce, 0)
}

/// Deterministic `Input::message_coin_predicate` sample used for round-trip testing.
pub fn sample_message_coin_predicate_input() -> Input {
    let sender = Address::new([14u8; 32]);
    let recipient = Address::new([15u8; 32]);
    let nonce = Nonce::new([16u8; 32]);
    Input::message_coin_predicate(
        sender,
        recipient,
        22,
        nonce,
        5,
        vec![0x01, 0x02],
        vec![0x03, 0x04],
    )
}

/// Deterministic `Input::message_data_signed` sample used for round-trip testing.
pub fn sample_message_data_signed_input() -> Input {
    let sender = Address::new([17u8; 32]);
    let recipient = Address::new([18u8; 32]);
    let nonce = Nonce::new([19u8; 32]);
    Input::message_data_signed(
        sender,
        recipient,
        23,
        nonce,
        1,
        vec![0xde, 0xad, 0xbe, 0xef],
    )
}

/// Deterministic `Input::message_data_predicate` sample used for round-trip testing.
pub fn sample_message_data_predicate_input() -> Input {
    let sender = Address::new([20u8; 32]);
    let recipient = Address::new([21u8; 32]);
    let nonce = Nonce::new([22u8; 32]);
    Input::message_data_predicate(
        sender,
        recipient,
        24,
        nonce,
        6,
        vec![0x99, 0x88],
        vec![0x77],
        vec![0x66],
    )
}

/// Collection of sample inputs covering every input variant.
pub fn sample_inputs() -> Vec<Input> {
    vec![
        sample_coin_signed_input(),
        sample_coin_predicate_input(),
        sample_contract_input(),
        sample_message_coin_signed_input(),
        sample_message_coin_predicate_input(),
        sample_message_data_signed_input(),
        sample_message_data_predicate_input(),
    ]
}

/// Collection of sample outputs covering every output variant.
pub fn sample_outputs() -> Vec<Output> {
    vec![
        Output::coin(Address::new([23u8; 32]), 50, AssetId::new([24u8; 32])),
        Output::Contract(OutputContract {
            input_index: 0,
            balance_root: Bytes32::from([25u8; 32]),
            state_root: Bytes32::from([26u8; 32]),
        }),
        Output::change(Address::new([27u8; 32]), 60, AssetId::new([28u8; 32])),
        Output::variable(Address::new([29u8; 32]), 70, AssetId::new([30u8; 32])),
        Output::contract_created(ContractId::new([31u8; 32]), Bytes32::from([32u8; 32])),
    ]
}

/// Sample `Transaction::Script` covering scripts, inputs, outputs, and witnesses.
pub fn sample_script_transaction() -> Transaction {
    let policies = Policies::new().with_witness_limit(10);
    let inputs = vec![
        sample_coin_signed_input(),
        sample_message_data_signed_input(),
    ];
    let outputs = vec![Output::coin(
        Address::new([40u8; 32]),
        11,
        AssetId::new([41u8; 32]),
    )];
    let witnesses = vec![Witness::from(vec![0x01, 0x02, 0x03])];
    let mut script = crate::fuel_tx::Transaction::script(
        1_000,
        vec![0x10, 0x20],
        vec![0x30, 0x40],
        policies,
        inputs,
        outputs,
        witnesses,
    );
    *script.receipts_root_mut() = Bytes32::from([33u8; 32]);
    Transaction::Script(script)
}

/// Sample `Transaction::Create` with deterministic storage slots and witnesses.
pub fn sample_create_transaction() -> Transaction {
    let policies = Policies::new();
    let storage_slots = vec![StorageSlot::new(
        Bytes32::from([34u8; 32]),
        Bytes32::from([35u8; 32]),
    )];
    let inputs = vec![sample_coin_signed_input()];
    let outputs = vec![Output::contract_created(
        ContractId::new([36u8; 32]),
        Bytes32::from([37u8; 32]),
    )];
    let witnesses = vec![Witness::from(vec![0xaa, 0xbb])];
    let create = crate::fuel_tx::Transaction::create(
        0,
        policies,
        crate::fuel_types::Salt::from([38u8; 32]),
        storage_slots,
        inputs,
        outputs,
        witnesses,
    );
    Transaction::Create(create)
}

/// Sample `Transaction::Mint` with deterministic contracts and asset data.
pub fn sample_mint_transaction() -> Transaction {
    let tx_pointer = TxPointer::new(BlockHeight::new(5), 0);
    let input_contract = InputContract {
        utxo_id: UtxoId::new(Bytes32::from([39u8; 32]), 3),
        balance_root: Bytes32::from([40u8; 32]),
        state_root: Bytes32::from([41u8; 32]),
        tx_pointer,
        contract_id: ContractId::new([42u8; 32]),
    };
    let output_contract = OutputContract {
        input_index: 0,
        balance_root: Bytes32::from([43u8; 32]),
        state_root: Bytes32::from([44u8; 32]),
    };
    let mint_asset_id = AssetId::new([45u8; 32]);
    let mint = crate::fuel_tx::Transaction::mint(
        tx_pointer,
        input_contract,
        output_contract,
        99,
        mint_asset_id,
        1,
    );
    Transaction::Mint(mint)
}

/// Sample `Transaction::Upgrade` using a state transition purpose.
pub fn sample_upgrade_transaction() -> Transaction {
    let policies = Policies::new();
    let inputs = vec![sample_coin_signed_input()];
    let outputs = vec![Output::coin(
        Address::new([46u8; 32]),
        5,
        AssetId::new([47u8; 32]),
    )];
    let witnesses = vec![Witness::from(vec![0x11, 0x22])];
    let purpose = UpgradePurpose::StateTransition {
        root: Bytes32::from([48u8; 32]),
    };
    let upgrade = crate::fuel_tx::Transaction::upgrade(
        purpose, policies, inputs, outputs, witnesses,
    );
    Transaction::Upgrade(upgrade)
}

/// Sample `Transaction::Upload` with deterministic proof set and witness index.
pub fn sample_upload_transaction() -> Transaction {
    let policies = Policies::new();
    let inputs = vec![sample_coin_signed_input()];
    let outputs = vec![Output::change(
        Address::new([49u8; 32]),
        3,
        AssetId::new([50u8; 32]),
    )];
    let witnesses = vec![Witness::from(vec![0x33, 0x44, 0x55])];
    let body = UploadBody {
        root: Bytes32::from([51u8; 32]),
        witness_index: 0,
        subsection_index: 0,
        subsections_number: 1,
        proof_set: vec![Bytes32::from([52u8; 32])],
    };
    let upload =
        crate::fuel_tx::Transaction::upload(body, policies, inputs, outputs, witnesses);
    Transaction::Upload(upload)
}

/// Sample `Transaction::Blob` using a computed blob ID and payload witness.
pub fn sample_blob_transaction() -> Transaction {
    let policies = Policies::new();
    let inputs = vec![sample_coin_signed_input()];
    let outputs = vec![Output::coin(
        Address::new([53u8; 32]),
        7,
        AssetId::new([54u8; 32]),
    )];
    let payload = vec![0x99, 0x00, 0x99];
    let witnesses = vec![Witness::from(payload.clone())];
    let blob_id = BlobId::compute(&payload);
    let body = BlobBody {
        id: blob_id,
        witness_index: 0,
    };
    let blob =
        crate::fuel_tx::Transaction::blob(body, policies, inputs, outputs, witnesses);
    Transaction::Blob(blob)
}

/// Collection of sample transactions covering every transaction variant.
pub fn sample_transactions() -> Vec<Transaction> {
    vec![
        sample_script_transaction(),
        sample_create_transaction(),
        sample_mint_transaction(),
        sample_upgrade_transaction(),
        sample_upload_transaction(),
        sample_blob_transaction(),
    ]
}

prop_compose! {
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

prop_compose! {
    /// Generate an arbitrary block with a variable number of transactions
    pub fn arb_block()(
        script_tx in arb_script_transaction(),
        da_height in any::<u64>(),
        consensus_parameter_version in any::<u32>(),
        state_transition_bytecode_version in any::<u32>(),
        msg_ids in arb_msg_ids(),
        event_root in any::<[u8; 32]>(),
        mut consensus_header in arb_consensus_header(),
    ) -> (Block, Vec<MessageId>, Bytes32) {
        let mut fuel_block = Block::default();

        let mut txs = sample_transactions();
        if !txs.is_empty() {
            txs[0] = script_tx;
        }

        // include txs first to be included in calculations
        *fuel_block.transactions_mut() = txs;

        fuel_block.header_mut().set_da_height(DaBlockHeight(da_height));
        fuel_block.header_mut().set_consensus_parameters_version(consensus_parameter_version);
        fuel_block.header_mut().set_state_transition_bytecode_version(state_transition_bytecode_version);

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
