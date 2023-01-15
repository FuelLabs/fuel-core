//! The resources module is an aggregation of all possible spendable entities(utxos, messages, etc).

use crate::{
    entities::{
        coin::Coin,
        message::Message,
    },
    fuel_asm::Word,
    fuel_tx::{
        AssetId,
        MessageId,
        UtxoId,
    },
};

/// The id of the resource.
#[allow(missing_docs)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceId {
    Utxo(UtxoId),
    Message(MessageId),
}

/// The primary type of spent or not spent resources(coins, messages, etc). The not spent resource
/// can be used as a source of information for the creation of the transaction's input.
#[allow(missing_docs)]
#[derive(Debug)]
pub enum Resource {
    Coin(Coin),
    Message(Message),
}

impl Resource {
    /// Return the amount of the resource.
    pub fn amount(&self) -> &Word {
        match self {
            Resource::Coin(coin) => &coin.amount,
            Resource::Message(message) => &message.amount,
        }
    }

    /// Return the `AssetId` of the resource.
    pub fn asset_id(&self) -> &AssetId {
        match self {
            Resource::Coin(coin) => &coin.asset_id,
            Resource::Message(_) => &AssetId::BASE,
        }
    }
}
