use fuel_core_types::{
    entities::{
        coins::coin::Coin,
        relayer::message::MessageV1,
        Message,
    },
    fuel_tx::{
        Address,
        AssetId,
    },
};

pub(crate) fn make_coin(owner: &Address, asset_id: &AssetId, amount: u64) -> Coin {
    Coin {
        utxo_id: Default::default(),
        owner: *owner,
        amount,
        asset_id: *asset_id,
        tx_pointer: Default::default(),
    }
}

pub(crate) fn make_retryable_message(owner: &Address, amount: u64) -> Message {
    Message::V1(MessageV1 {
        sender: Default::default(),
        recipient: *owner,
        nonce: Default::default(),
        amount,
        data: vec![1],
        da_height: Default::default(),
    })
}

pub(crate) fn make_nonretryable_message(owner: &Address, amount: u64) -> Message {
    let mut message = make_retryable_message(owner, amount);
    message.set_data(vec![]);
    message
}
