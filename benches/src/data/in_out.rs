use crate::Database;
use fuel_core_types::{
    fuel_asm::op,
    fuel_tx::{
        Input,
        Output,
        Witness,
    },
    fuel_types::{
        Address,
        AssetId,
        Nonce,
        Word,
    },
    fuel_vm::SecretKey,
};

use super::Data;
pub use helpers::*;

#[cfg(test)]
mod tests;

mod helpers;

/// All possible input types.
pub mod in_ty {
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

/// All possible output types.
pub mod out_ty {
    #[derive(Default, Debug, Clone, Copy)]
    pub struct Void;
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
}

#[derive(Default)]
/// Holds the input and output data for a transaction.
/// Also holds the witnesses and secrets for signing the inputs.
pub struct InputOutputData {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub witnesses: Vec<Witness>,
    pub secrets: Vec<SecretKey>,
}

#[derive(Default, Debug, Clone)]
/// Common data for all input message types.
struct MessageInner {
    sender: Address,
    recipient: Address,
    nonce: Nonce,
    amount: Word,
}

#[derive(Default, Debug, Clone)]
/// A signed message coin.
struct SignedMessageCoin {
    secret: SecretKey,
    inner: MessageInner,
}

/// A valid set of inputs and outputs for a transaction.
pub trait ValidTx<I> {
    /// Fill the input and output data with the given number of inputs and outputs.
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize);
}

impl ValidTx<in_ty::CoinSigned> for out_ty::Void {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize) {
        for (coin, secret) in coins_signed(from, to.witness_index()).take(num) {
            to.insert_input(coin, secret);
        }
    }
}

impl ValidTx<in_ty::CoinPredicate> for out_ty::Void {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize) {
        let mut predicate_data = from.data_range(100..(100 + num));
        for _ in 0..num {
            let coin = coin_predicate(from, predicate_data.by_ref());
            to.inputs.push(coin);
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

impl ValidTx<in_ty::CoinSigned> for out_ty::Coin {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize) {
        let input_coins: Vec<_> =
            coins_signed(from, to.witness_index()).take(num).collect();
        for (coin, secret) in input_coins {
            to.outputs.push(output_coin(
                from,
                coin.amount().unwrap(),
                *coin.asset_id().unwrap(),
            ));
            to.insert_input(coin, secret);
        }
    }
}

impl ValidTx<in_ty::CoinSigned> for out_ty::Variable {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize) {
        let input_coins: Vec<_> =
            coins_signed(from, to.witness_index()).take(num).collect();
        for (coin, secret) in input_coins {
            to.outputs.push(output_variable(
                from,
                coin.amount().unwrap(),
                *coin.asset_id().unwrap(),
            ));
            to.insert_input(coin, secret);
        }
    }
}

impl ValidTx<in_ty::CoinSigned> for out_ty::Change {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize) {
        let input_coins: Vec<_> =
            coins_signed(from, to.witness_index()).take(num).collect();
        for (coin, secret) in input_coins {
            to.outputs.push(output_change(
                from,
                coin.amount().unwrap(),
                *coin.asset_id().unwrap(),
            ));
            to.insert_input(coin, secret);
        }
    }
}

impl ValidTx<in_ty::MessageCoinSigned> for out_ty::Void {
    fn fill(from: &mut Data, to: &mut InputOutputData, num: usize) {
        for (coin, secret) in message_coins_signed(from, to.witness_index()).take(num) {
            to.insert_input(coin, secret);
        }
    }
}

/// Helper function to get the owner of a secret key.
fn owner(secret: &SecretKey) -> Address {
    Input::owner(&secret.public_key())
}

impl InputOutputData {
    /// Insert and sign an input.
    fn insert_input(&mut self, input: Input, secret: SecretKey) {
        self.inputs.push(input);
        self.witnesses.push(Witness::default());
        self.secrets.push(secret);
    }

    /// Get the next witness index.
    fn witness_index(&self) -> u8 {
        self.witnesses.len() as u8
    }
}

fn coins_signed(
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

fn message_coins_signed(
    data: &mut Data,
    mut witness_index: u8,
) -> impl Iterator<Item = (Input, SecretKey)> + '_ {
    std::iter::repeat_with(move || {
        let message = signed_message_coin(data);

        let input = Input::message_coin_signed(
            message.inner.sender,
            message.inner.recipient,
            message.inner.amount,
            message.inner.nonce,
            witness_index,
        );
        witness_index += 1;
        (input, message.secret)
    })
}

fn signed_message_coin(data: &mut Data) -> SignedMessageCoin {
    let amount = data.word();
    let secret = data.secret_key();
    let recipient = Input::owner(&secret.public_key());
    SignedMessageCoin {
        secret,
        inner: MessageInner {
            amount,
            sender: data.address(),
            recipient,
            nonce: data.nonce(),
        },
    }
}

fn message_data(
    data: &mut Data,
    msg_data: &mut impl Iterator<Item = Vec<u8>>,
    predicate_data: &mut impl Iterator<Item = Vec<u8>>,
) -> Input {
    let msg = signed_message_coin(data);
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

fn coin_predicate(
    data: &mut Data,
    predicate_data: &mut impl Iterator<Item = Vec<u8>>,
) -> Input {
    let predicate: Vec<u8> = [op::ret(1)].into_iter().collect();
    Input::coin_predicate(
        data.utxo_id(),
        Input::predicate_owner(&predicate, &Default::default()),
        data.word(),
        data.asset_id(),
        Default::default(),
        Default::default(),
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

fn output_coin(data: &mut Data, amount: Word, asset_id: AssetId) -> Output {
    Output::coin(data.address(), amount, asset_id)
}

fn output_variable(data: &mut Data, amount: Word, asset_id: AssetId) -> Output {
    Output::variable(data.address(), amount, asset_id)
}

fn output_change(data: &mut Data, amount: Word, asset_id: AssetId) -> Output {
    Output::change(data.address(), amount, asset_id)
}
