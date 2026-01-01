#[cfg(feature = "fault-proving")]
use crate::fuel_types::ChainId;
use crate::{
    blockchain::{
        block::Block,
        header::{
            ApplicationHeader,
            BlockHeader,
            BlockHeaderV1,
            PartialBlockHeader,
            generate_txns_root,
            v1::GeneratedApplicationFieldsV1,
        },
    },
    fuel_asm::PanicInstruction,
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
        Receipt,
        ScriptExecutionResult,
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
        BlobId,
        BlockHeight,
        Nonce,
        SubAssetId,
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

fn arb_contract_id() -> impl Strategy<Value = ContractId> {
    any::<[u8; 32]>().prop_map(ContractId::new)
}

fn arb_sub_asset_id() -> impl Strategy<Value = SubAssetId> {
    any::<[u8; 32]>().prop_map(SubAssetId::new)
}

fn arb_panic_instruction() -> impl Strategy<Value = PanicInstruction> {
    any::<u64>().prop_map(PanicInstruction::from)
}

fn arb_script_execution_result() -> impl Strategy<Value = ScriptExecutionResult> {
    prop_oneof![
        Just(ScriptExecutionResult::Success),
        Just(ScriptExecutionResult::Revert),
        Just(ScriptExecutionResult::Panic),
        any::<u64>().prop_map(ScriptExecutionResult::GenericFailure),
    ]
}

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
                let create = Transaction::create(
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
                let mint = Transaction::mint(
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
    (
        arb_policies(),
        arb_inputs(),
        arb_outputs(),
        prop::collection::vec(arb_witness(), 1..4),
        any::<[u8; 32]>(),
    )
        .prop_map(|(policies, inputs, outputs, witnesses, checksum_bytes)| {
            let purpose = UpgradePurpose::ConsensusParameters {
                witness_index: 0,
                checksum: checksum_bytes.into(),
            };
            let upgrade = crate::fuel_tx::Transaction::upgrade(
                purpose, policies, inputs, outputs, witnesses,
            );
            Transaction::Upgrade(upgrade)
        })
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

prop_compose! {
    /// Generate an arbitrary block with a variable number of transactions
    pub fn arb_block()(
        txs in arb_txs(),
        da_height in any::<u64>(),
        consensus_parameters_version in any::<u32>(),
        state_transition_bytecode_version in any::<u32>(),
        msg_ids in arb_msg_ids(),
        event_root in any::<[u8; 32]>(),
        chain_id in any::<u64>(),
        receipts in arb_receipts()
    ) -> (Block, Vec<Receipt>) {
        let transactions_count = txs.len().try_into().expect("we shouldn't have more than u16::MAX transactions");
        let message_receipt_count = msg_ids.len().try_into().expect("we shouldn't have more than u32::MAX messages");
        let transactions_root = generate_txns_root(&txs);
        let message_outbox_root = msg_ids
            .iter()
            .fold(MerkleRootCalculator::new(), |mut tree, id| {
                tree.push(id.as_ref());
                tree
            })
            .root()
            .into();

        let mut msg_ids = Vec::new();
        let reverted = receipts
            .iter()
            .any(|r| matches!(r, Receipt::Revert { .. } | Receipt::Panic { .. }));

        if !reverted {
            msg_ids.extend(receipts.iter().filter_map(|r| r.message_id()));
        }

        let event_root: Bytes32 = event_root.into();
        let header = {
            let mut default = BlockHeaderV1::default();
                            default.set_application_header(ApplicationHeader {
                                da_height: da_height.into(),
                                consensus_parameters_version,
                                state_transition_bytecode_version,
                                generated: GeneratedApplicationFieldsV1 {
                                    transactions_count,
                                    message_receipt_count,
                                    transactions_root,
                                    message_outbox_root,
                                    event_inbox_root: event_root,
                                },
                            });

                            BlockHeader::V1(default)
                        };
        let partial_block_header = PartialBlockHeader::from(&header);
        #[cfg(feature = "fault-proving")]
        let fuel_block = {
            let chain_id = ChainId::new(chain_id);
            Block::new(partial_block_header, txs, &msg_ids, event_root, &chain_id).unwrap()
        };
        #[cfg(not(feature = "fault-proving"))]
        let fuel_block = {
            let _ = chain_id;
            Block::new(partial_block_header, txs, &msg_ids, event_root).unwrap()
        };
        (fuel_block, receipts)
    }
}

fn arb_receipt() -> impl Strategy<Value = Receipt> {
    prop_oneof![
        (
            arb_contract_id(),
            arb_contract_id(),
            any::<u64>(),
            arb_asset_id(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(
                |(id, to, amount, asset_id, gas, param1, param2, pc, is)| {
                    Receipt::call(id, to, amount, asset_id, gas, param1, param2, pc, is)
                },
            ),
        (arb_contract_id(), any::<u64>(), any::<u64>(), any::<u64>(),)
            .prop_map(|(id, val, pc, is)| Receipt::ret(id, val, pc, is)),
        (
            arb_contract_id(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            prop::collection::vec(any::<u8>(), 0..64),
        )
            .prop_map(|(id, ptr, pc, is, data)| Receipt::return_data(
                id, ptr, pc, is, data,
            )),
        (
            arb_contract_id(),
            arb_panic_instruction(),
            any::<u64>(),
            any::<u64>(),
            prop::option::of(arb_contract_id()),
        )
            .prop_map(|(id, reason, pc, is, panic_contract)| {
                Receipt::panic(id, reason, pc, is).with_panic_contract_id(panic_contract)
            }),
        (arb_contract_id(), any::<u64>(), any::<u64>(), any::<u64>(),)
            .prop_map(|(id, ra, pc, is)| Receipt::revert(id, ra, pc, is)),
        (
            arb_contract_id(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(|(id, ra, rb, rc, rd, pc, is)| {
                Receipt::log(id, ra, rb, rc, rd, pc, is)
            }),
        (
            arb_contract_id(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            prop::collection::vec(any::<u8>(), 0..64),
        )
            .prop_map(|(id, ra, rb, ptr, pc, is, data)| {
                Receipt::log_data(id, ra, rb, ptr, pc, is, data)
            }),
        (
            arb_contract_id(),
            arb_contract_id(),
            any::<u64>(),
            arb_asset_id(),
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(|(id, to, amount, asset_id, pc, is)| {
                Receipt::transfer(id, to, amount, asset_id, pc, is)
            }),
        (
            arb_contract_id(),
            arb_address(),
            any::<u64>(),
            arb_asset_id(),
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(|(id, to, amount, asset_id, pc, is)| {
                Receipt::transfer_out(id, to, amount, asset_id, pc, is)
            }),
        (arb_script_execution_result(), any::<u64>())
            .prop_map(|(result, gas_used)| Receipt::script_result(result, gas_used),),
        (
            arb_address(),
            arb_address(),
            any::<u64>(),
            arb_nonce(),
            prop::collection::vec(any::<u8>(), 0..64),
        )
            .prop_map(|(sender, recipient, amount, nonce, data)| {
                let len = data.len() as u64;
                let digest = Output::message_digest(&data);
                Receipt::message_out_with_len(
                    sender,
                    recipient,
                    amount,
                    nonce,
                    len,
                    digest,
                    Some(data),
                )
            }),
        (
            arb_sub_asset_id(),
            arb_contract_id(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(|(sub_id, contract_id, val, pc, is)| {
                Receipt::mint(sub_id, contract_id, val, pc, is)
            }),
        (
            arb_sub_asset_id(),
            arb_contract_id(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(|(sub_id, contract_id, val, pc, is)| {
                Receipt::burn(sub_id, contract_id, val, pc, is)
            }),
    ]
}

prop_compose! {
    /// generates a list of random receipts
    pub fn arb_receipts()(
        receipts in prop::collection::vec(arb_receipt(), 0..10),
    ) -> Vec<Receipt> {
        receipts
    }
}
