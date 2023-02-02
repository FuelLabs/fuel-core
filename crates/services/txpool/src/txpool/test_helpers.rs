use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    entities::message::Message,
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
    spent_block: Option<BlockHeight>,
) -> (Message, Input) {
    let predicate = vec![op::ret(1)].into_iter().collect::<Vec<u8>>();
    let message = Message {
        sender: Default::default(),
        recipient: Input::predicate_owner(&predicate),
        nonce: 0,
        amount,
        data: vec![],
        da_height: Default::default(),
        fuel_block_spend: spent_block,
    };

    (
        message.clone(),
        Input::message_predicate(
            message.id(),
            message.sender,
            Input::predicate_owner(&predicate),
            message.amount,
            message.nonce,
            message.data,
            predicate,
            Default::default(),
        ),
    )
}

pub(crate) fn create_coin_output() -> Output {
    Output::Coin {
        amount: Default::default(),
        to: Default::default(),
        asset_id: Default::default(),
    }
}

pub(crate) fn create_contract_input(tx_id: TxId, output_index: u8) -> Input {
    Input::Contract {
        utxo_id: UtxoId::new(tx_id, output_index),
        balance_root: Default::default(),
        state_root: Default::default(),
        tx_pointer: Default::default(),
        contract_id: Default::default(),
    }
}

pub(crate) fn create_contract_output(contract_id: ContractId) -> Output {
    Output::ContractCreated {
        contract_id,
        state_root: Contract::default_state_root(),
    }
}
