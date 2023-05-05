use std::collections::HashSet;

use fuel_core_benches::Database;
use fuel_core_types::{
    fuel_tx::{
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

#[derive(Default, Debug, Clone)]
struct InputToOutput {
    coins_to_void: usize,
    coins_to_coin: usize,
    coins_to_change: usize,
    coins_to_variable: usize,
    coins_to_contract_created: usize,
    messages_to_void: usize,
    messages_to_coin: usize,
    messages_to_change: usize,
    messages_to_variable: usize,
    messages_to_contract_created: usize,
    contracts_to_contract: usize,
    void_to_message: usize,
    // void_to_coin: usize,
    void_to_change: HashSet<AssetId>,
    void_to_variable: usize,
    void_contract_created: usize,
}

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
struct SignedMessage {
    secret: SecretKey,
    sender: Address,
    recipient: Address,
    nonce: Nonce,
    amount: Word,
    data: Vec<u8>,
}

impl InputOutputData {
    fn extend(&mut self, data: &mut Data, params: &InputToOutput) {
        for coin in coins_to_void(data).take(params.coins_to_void) {
            self.insert_coin(coin);
        }
        for (coin, output) in coins_to_coin(data).take(params.coins_to_coin) {
            self.insert_coin(coin);
            self.outputs.push(output);
        }
        for (coin, output) in coins_to_change(data).take(params.coins_to_change) {
            self.insert_coin(coin);
            self.outputs.push(output);
        }
        for (coin, output) in coins_to_variable(data).take(params.coins_to_variable) {
            self.insert_coin(coin);
            self.outputs.push(output);
        }
        for msg in messages_to_void(data).take(params.messages_to_void) {
            self.insert_message(msg);
        }
        for (msg, output) in messages_to_coin(data).take(params.messages_to_coin) {
            self.insert_message(msg);
            self.outputs.push(output);
        }
        for (msg, output) in messages_to_change(data).take(params.messages_to_change) {
            self.insert_message(msg);
            self.outputs.push(output);
        }
        for (msg, output) in messages_to_variable(data).take(params.messages_to_variable)
        {
            self.insert_message(msg);
            self.outputs.push(output);
        }
    }

    fn insert_coin(&mut self, coin: SignedCoin) {
        let owner = Input::owner(&coin.secret.public_key());
        let witness_idx = self.witnesses.len() as u8;
        let input = Input::coin_signed(
            coin.utxo_id,
            owner,
            coin.amount,
            coin.asset_id,
            coin.tx_pointer,
            witness_idx,
            coin.maturity,
        );
        self.inputs.push(input);
        self.witnesses.push(Witness::default());
        self.secrets.push(coin.secret);
    }

    fn insert_message(&mut self, message: SignedMessage) {
        let witness_idx = self.witnesses.len() as u8;
        let input = if !message.data.is_empty() {
            Input::message_data_signed(
                message.sender,
                message.recipient,
                message.amount,
                message.nonce,
                witness_idx,
                message.data,
            )
        } else {
            Input::message_coin_signed(
                message.sender,
                message.recipient,
                message.amount,
                message.nonce,
                witness_idx,
            )
        };
        self.inputs.push(input);
        self.witnesses.push(Witness::default());
        self.secrets.push(message.secret);
    }
}

// fn void_to_coin(data: &mut Data) -> impl Iterator<Item = Output> + '_ {
//     std::iter::repeat_with(|| Output::coin(data.address(), data.word(), data.asset_id()))
// }

fn coins_to_void(data: &mut Data) -> impl Iterator<Item = SignedCoin> + '_ {
    std::iter::repeat_with(|| SignedCoin {
        secret: data.secret_key(),
        utxo_id: data.utxo_id(),
        amount: data.word(),
        asset_id: data.asset_id(),
        tx_pointer: Default::default(),
        maturity: Default::default(),
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

fn message(data: &mut Data) -> (AssetId, Word, SignedMessage) {
    let amount = data.word();
    let message = SignedMessage {
        secret: data.secret_key(),
        amount,
        sender: data.address(),
        recipient: data.address(),
        nonce: data.nonce(),
        data: Vec::with_capacity(0),
    };
    (AssetId::BASE, amount, message)
}

fn messages_to_void(data: &mut Data) -> impl Iterator<Item = SignedMessage> + '_ {
    std::iter::repeat_with(|| message(data).2)
}

fn messages_to_coin(
    data: &mut Data,
) -> impl Iterator<Item = (SignedMessage, Output)> + '_ {
    std::iter::repeat_with(|| {
        let (asset_id, amount, message) = message(data);
        let output = Output::coin(data.address(), amount, asset_id);
        (message, output)
    })
}

fn messages_to_change(
    data: &mut Data,
) -> impl Iterator<Item = (SignedMessage, Output)> + '_ {
    std::iter::repeat_with(|| {
        let (asset_id, amount, message) = message(data);
        let output = Output::change(data.address(), amount, asset_id);
        (message, output)
    })
}

fn messages_to_variable(
    data: &mut Data,
) -> impl Iterator<Item = (SignedMessage, Output)> + '_ {
    std::iter::repeat_with(|| {
        let (asset_id, amount, message) = message(data);
        let output = Output::variable(data.address(), amount, asset_id);
        (message, output)
    })
}
