use crate::test_helpers::IntoEstimated;
use fuel_core_types::{
    entities::relayer::message::{
        Message,
        MessageV1,
    },
    fuel_asm::op,
    fuel_tx::{
        Contract,
        ContractId,
        Input,
        Output,
        TxId,
        UtxoId,
    },
    fuel_types::Word,
};

pub(crate) fn create_message_predicate_from_message(
    amount: Word,
    nonce: u64,
) -> (Message, Input) {
    let predicate = vec![op::ret(1)].into_iter().collect::<Vec<u8>>();
    let message = MessageV1 {
        sender: Default::default(),
        recipient: Input::predicate_owner(&predicate),
        nonce: nonce.into(),
        amount,
        data: vec![],
        da_height: Default::default(),
    };

    (
        message.clone().into(),
        Input::message_coin_predicate(
            message.sender,
            Input::predicate_owner(&predicate),
            message.amount,
            message.nonce,
            Default::default(),
            predicate,
            Default::default(),
        )
        .into_default_estimated(),
    )
}

pub(crate) fn create_coin_output() -> Output {
    Output::coin(Default::default(), Default::default(), Default::default())
}

pub(crate) fn create_contract_input(
    tx_id: TxId,
    output_index: u16,
    contract_id: ContractId,
) -> Input {
    Input::contract(
        UtxoId::new(tx_id, output_index),
        Default::default(),
        Default::default(),
        Default::default(),
        contract_id,
    )
}

pub(crate) fn create_contract_output(contract_id: ContractId) -> Output {
    Output::contract_created(contract_id, Contract::default_state_root())
}
