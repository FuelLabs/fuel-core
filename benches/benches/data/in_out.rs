use std::collections::HashSet;

use fuel_core_benches::Database;
use fuel_core_types::{
    fuel_asm::op,
    fuel_tx::{
        input::contract::Contract,
        Input,
        Output,
        TxPointer,
        UtxoId,
        Witness,
    },
    fuel_types::{
        Address,
        AssetId,
        BlockHeight,
        Nonce,
        Word,
    },
    fuel_vm::SecretKey,
};

use enum_iterator::{
    all,
    Sequence,
};

use super::Data;

#[cfg(test)]
mod tests;

// #[derive(Default, Debug, Clone)]
// struct InputToOutput {
//     coins_to_void: usize,
//     coins_to_coin: usize,
//     coins_to_change: usize,
//     coins_to_variable: usize,
//     coins_to_contract_created: usize,
//     messages_to_void: usize,
//     messages_to_coin: usize,
//     messages_to_change: usize,
//     messages_to_variable: usize,
//     messages_to_contract_created: usize,
//     contracts_to_contract: usize,
//     void_to_coin: usize,
//     void_to_message: usize,
//     void_to_change: HashSet<AssetId>,
//     void_to_variable: usize,
//     void_contract_created: usize,
// }

mod in_ty {
    #[derive(Default, Debug, Clone, Copy)]
    pub struct CoinSigned;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct CoinPredicate;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct MessageCoinSigned;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct MessageCoinPredicate;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct MessageData;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct Contract;
}

mod out_ty {
    #[derive(Default, Debug, Clone, Copy)]
    pub struct Coin;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct Contract;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct Message;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct Change;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct Variable;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct ContractCreated;
    #[derive(Default, Debug, Clone, Copy)]
    pub struct Void;
}

trait ValidTx<I> {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize);
}

impl ValidTx<in_ty::CoinSigned> for out_ty::Void {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize) {
        for (coin, secret) in coins_signed_to_void(from, to.witness_index()).take(num) {
            to.insert_input(coin, secret);
        }
    }
}

impl ValidTx<(in_ty::MessageData, in_ty::Contract)> for (out_ty::Coin, out_ty::Contract) {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize) {
        let mut data_iter = from.data_range(100..(100 + num));
        let mut predicate_data = from.data_range(100..(100 + num));
        for _ in 0..num {
            let msg = message_data(from, data_iter.by_ref(), predicate_data.by_ref());
            to.inputs.push(msg);
            to.inputs.push(input_contract(from));
            to.outputs
                .push(output_contract(from, (to.inputs.len() - 1) as u8));
        }
    }
}

fn owner(secret: &SecretKey) -> Address {
    Input::owner(&secret.public_key())
}
// impl ValidTx<in_ty::CoinSigned> for out_ty::Coin {}

#[derive(Default)]
struct InputOutputData {
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    witnesses: Vec<Witness>,
    secrets: Vec<SecretKey>,
}

#[derive(Default, Debug, Clone)]
struct SignedCoin {
    secret: SecretKey,
    utxo_id: UtxoId,
    amount: Word,
    asset_id: AssetId,
    tx_pointer: TxPointer,
    maturity: BlockHeight,
}

#[derive(Default, Debug, Clone)]
struct MessageInner {
    sender: Address,
    recipient: Address,
    nonce: Nonce,
    amount: Word,
}

#[derive(Default, Debug, Clone)]
struct SignedMessageCoin {
    secret: SecretKey,
    inner: MessageInner,
}

#[derive(Default, Debug, Clone)]
struct PredicateMessageCoin {
    inner: MessageInner,
}

#[derive(Default, Debug, Clone)]
struct MessageData {
    inner: MessageInner,
    data: Vec<u8>,
    predicate_data: Vec<u8>,
}

impl InputOutputData {
    fn insert_input(&mut self, input: Input, secret: SecretKey) {
        self.inputs.push(input);
        self.witnesses.push(Witness::default());
        self.secrets.push(secret);
    }

    fn witness_index(&self) -> u8 {
        self.witnesses.len() as u8
    }
}

fn void_to_coin(data: &mut Data) -> impl Iterator<Item = Output> + '_ {
    std::iter::repeat_with(|| Output::coin(data.address(), data.word(), data.asset_id()))
}

fn coins_signed_to_void(
    data: &mut Data,
    mut witness_index: u8,
) -> impl Iterator<Item = (Input, SecretKey)> + '_ {
    std::iter::repeat_with(move || {
        let secret = data.secret_key();
        let input = Input::coin_signed(
            data.utxo_id(),
            owner(&secret),
            data.word(),
            data.asset_id(),
            Default::default(),
            witness_index,
            Default::default(),
        );
        witness_index += 1;
        (input, secret)
    })
}

fn coin(data: &mut Data) -> (AssetId, Word, SignedCoin) {
    let asset_id = data.asset_id();
    let amount = data.word();
    let coin = SignedCoin {
        secret: data.secret_key(),
        utxo_id: data.utxo_id(),
        amount,
        asset_id,
        tx_pointer: Default::default(),
        maturity: Default::default(),
    };
    (asset_id, amount, coin)
}

fn coins_to_coin(data: &mut Data) -> impl Iterator<Item = (SignedCoin, Output)> + '_ {
    std::iter::repeat_with(|| {
        let (asset_id, amount, coin) = coin(data);
        let output = Output::coin(data.address(), amount, asset_id);
        (coin, output)
    })
}

fn coins_to_change(data: &mut Data) -> impl Iterator<Item = (SignedCoin, Output)> + '_ {
    std::iter::repeat_with(|| {
        let (asset_id, amount, coin) = coin(data);
        let output = Output::change(data.address(), amount, asset_id);
        (coin, output)
    })
}

fn coins_to_variable(data: &mut Data) -> impl Iterator<Item = (SignedCoin, Output)> + '_ {
    std::iter::repeat_with(|| {
        let (asset_id, amount, coin) = coin(data);
        let output = Output::variable(data.address(), amount, asset_id);
        (coin, output)
    })
}

fn signed_message_coin(data: &mut Data) -> (AssetId, Word, SignedMessageCoin) {
    let amount = data.word();
    let secret = data.secret_key();
    let recipient = Input::owner(&secret.public_key());
    let message = SignedMessageCoin {
        secret,
        inner: MessageInner {
            amount,
            sender: data.address(),
            recipient,
            nonce: data.nonce(),
        },
    };
    (AssetId::BASE, amount, message)
}

fn message_data(
    data: &mut Data,
    msg_data: &mut impl Iterator<Item = Vec<u8>>,
    predicate_data: &mut impl Iterator<Item = Vec<u8>>,
) -> Input {
    let (_, _, msg) = signed_message_coin(data);
    let predicate: Vec<u8> = [op::ret(1)].into_iter().collect();
    Input::message_data_predicate(
        msg.inner.sender,
        Input::predicate_owner(&predicate, &Default::default()),
        msg.inner.amount,
        msg.inner.nonce,
        msg_data.next().unwrap(),
        predicate,
        predicate_data.next().unwrap(),
    )
}

fn input_contract(data: &mut Data) -> Input {
    Input::contract(
        data.utxo_id(),
        data.bytes32(),
        data.bytes32(),
        Default::default(),
        data.contract_id(),
    )
}

fn output_contract(data: &mut Data, input_idx: u8) -> Output {
    Output::contract(input_idx, data.bytes32(), data.bytes32())
}

// fn messages_to_void(data: &mut Data) -> impl Iterator<Item = SignedMessage> + '_ {
//     std::iter::repeat_with(|| message(data).2)
// }

// fn messages_to_coin(
//     data: &mut Data,
// ) -> impl Iterator<Item = (SignedMessage, Output)> + '_ {
//     std::iter::repeat_with(|| {
//         let (asset_id, amount, message) = message(data);
//         let output = Output::coin(data.address(), amount, asset_id);
//         (message, output)
//     })
// }

// fn messages_to_change(
//     data: &mut Data,
// ) -> impl Iterator<Item = (SignedMessage, Output)> + '_ {
//     std::iter::repeat_with(|| {
//         let (_, amount, message) = message(data);
//         let output = Output::change(data.address(), amount, data.asset_id());
//         (message, output)
//     })
// }

// fn messages_to_variable(
//     data: &mut Data,
// ) -> impl Iterator<Item = (SignedMessage, Output)> + '_ {
//     std::iter::repeat_with(|| {
//         let (asset_id, amount, message) = message(data);
//         let output = Output::variable(data.address(), amount, asset_id);
//         (message, output)
//     })
// }
