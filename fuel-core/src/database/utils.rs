use crate::database::Database;
use fuel_core_interfaces::common::fuel_storage::Storage;
use fuel_core_interfaces::common::fuel_tx::UtxoId;
use fuel_core_interfaces::common::fuel_types::{Address, AssetId, MessageId, Word};
use fuel_core_interfaces::db::{Error, KvStoreError};
use fuel_core_interfaces::model::{Coin, CoinStatus, Message};
use itertools::Itertools;
use std::borrow::Cow;
use std::collections::HashSet;

const BASE_ASSET: AssetId = AssetId::zeroed();

/// At least required `target` of the query per asset's `id`.
pub struct Asset {
    pub id: AssetId,
    pub target: u64,
}

impl Asset {
    pub fn new(id: AssetId, target: u64) -> Self {
        Self { id, target }
    }
}

#[derive(Default)]
pub struct Excluder {
    pub utxos: HashSet<UtxoId>,
    pub messages: HashSet<MessageId>,
}

impl Excluder {
    pub fn new(ids: Vec<BanknoteId>) -> Self {
        let mut instance = Self::default();

        for id in ids.into_iter() {
            match id {
                BanknoteId::Utxo(utxo) => instance.utxos.insert(utxo),
                BanknoteId::Message(message) => instance.messages.insert(message),
            };
        }

        instance
    }
}

pub struct AssetQuery<'a> {
    pub owner: &'a Address,
    pub asset: &'a Asset,
    pub excluder: Option<&'a Excluder>,
    pub database: &'a Database,
}

impl<'a> AssetQuery<'a> {
    pub fn new(
        owner: &'a Address,
        asset: &'a Asset,
        excluder: Option<&'a Excluder>,
        database: &'a Database,
    ) -> Self {
        Self {
            owner,
            asset,
            excluder,
            database,
        }
    }

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) inputs of the `owner`
    /// for the `asset_id`.
    // TODO: Optimize this by creating an index
    pub fn unspent_inputs(
        &self,
    ) -> impl Iterator<Item = Result<Banknote<Cow<Coin>, Cow<Message>>, Error>> + '_ {
        let coins_iter = self
            .database
            .owned_coins_utxos(self.owner, None, None)
            .filter_ok(|id| {
                if let Some(excluder) = self.excluder {
                    !excluder.utxos.contains(id)
                } else {
                    true
                }
            })
            .map(|res| {
                res.map(|id| {
                    let coin = Storage::<UtxoId, Coin>::get(self.database, &id)?
                        .ok_or(KvStoreError::NotFound)?;

                    Ok::<_, KvStoreError>(Banknote::Coin { id, fields: coin })
                })
            })
            .map(|results| Ok(results??))
            .filter_ok(|coin| {
                if let Banknote::Coin { fields, .. } = coin {
                    fields.asset_id == self.asset.id && fields.status == CoinStatus::Unspent
                } else {
                    true
                }
            });

        // TODO: If asset_id is zero, also check messages.

        coins_iter
    }
}

/// The id of the banknote.
pub enum BanknoteId {
    Utxo(UtxoId),
    Message(MessageId),
}

/// The primary type of spent or not spent banknotes(coins, messages, etc). The not spent banknote
/// can be used as a source of information for the creation of the transaction's input.
#[derive(Debug)]
pub enum Banknote<C, M> {
    Coin { id: UtxoId, fields: C },
    Message { id: MessageId, fields: M },
}

impl<C: AsRef<Coin>, M: AsRef<Message>> Banknote<C, M> {
    pub fn amount(&self) -> &Word {
        match self {
            Banknote::Coin { fields, .. } => &fields.as_ref().amount,
            Banknote::Message { fields, .. } => &fields.as_ref().amount,
        }
    }

    pub fn asset_id(&self) -> &AssetId {
        match self {
            Banknote::Coin { fields, .. } => &fields.as_ref().asset_id,
            Banknote::Message { .. } => &BASE_ASSET,
        }
    }
}

impl<'c, 'm, C, M> Banknote<Cow<'c, C>, Cow<'m, M>>
where
    C: Clone + AsRef<Coin>,
    M: Clone + AsRef<Message>,
{
    /// Return owned `C` or `M`. Will clone if is borrowed.
    pub fn into_owned(self) -> Banknote<C, M> {
        match self {
            Banknote::Coin { id, fields } => Banknote::Coin {
                id,
                fields: fields.into_owned(),
            },
            Banknote::Message { id, fields } => Banknote::Message {
                id,
                fields: fields.into_owned(),
            },
        }
    }
}
