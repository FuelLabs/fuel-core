use crate::{
    fuel_core_graphql_api::service::DatabaseTemp,
    query::{
        CoinQueryContext,
        MessageQueryContext,
    },
    state::IterDirection,
};
use fuel_core_storage::{
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_types::{
    entities::{
        coin::{
            Coin,
            CoinStatus,
        },
        message::Message,
    },
    fuel_tx::UtxoId,
    fuel_types::{
        Address,
        AssetId,
        MessageId,
        Word,
    },
};
use itertools::Itertools;
use std::{
    borrow::{
        Borrow,
        Cow,
    },
    collections::HashSet,
};

/// At least required `target` of the query per asset's `id` with `max` resources.
#[derive(Clone)]
pub struct AssetSpendTarget {
    pub id: AssetId,
    pub target: u64,
    pub max: usize,
}

impl AssetSpendTarget {
    pub fn new(id: AssetId, target: u64, max: u64) -> Self {
        Self {
            id,
            target,
            max: max as usize,
        }
    }
}

#[derive(Default)]
pub struct Exclude {
    pub utxos: HashSet<UtxoId>,
    pub messages: HashSet<MessageId>,
}

impl Exclude {
    pub fn new(ids: Vec<ResourceId>) -> Self {
        let mut instance = Self::default();

        for id in ids.into_iter() {
            match id {
                ResourceId::Utxo(utxo) => instance.utxos.insert(utxo),
                ResourceId::Message(message) => instance.messages.insert(message),
            };
        }

        instance
    }
}

pub struct AssetsQuery<'a> {
    pub owner: &'a Address,
    pub assets: Option<HashSet<&'a AssetId>>,
    pub exclude: Option<&'a Exclude>,
    pub database: &'a DatabaseTemp,
}

impl<'a> AssetsQuery<'a> {
    pub fn new(
        owner: &'a Address,
        assets: Option<HashSet<&'a AssetId>>,
        exclude: Option<&'a Exclude>,
        database: &'a DatabaseTemp,
    ) -> Self {
        Self {
            owner,
            assets,
            exclude,
            database,
        }
    }

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) resources of the `owner`.
    ///
    /// # Note: The resources of different type are not grouped by the `asset_id`.
    // TODO: Optimize this by creating an index
    //  https://github.com/FuelLabs/fuel-core/issues/588
    pub fn unspent_resources(
        &self,
    ) -> impl Iterator<Item = StorageResult<Resource<Coin, Message>>> + '_ {
        let coins_iter = CoinQueryContext(self.database)
            .owned_coins_ids(self.owner, None, IterDirection::Forward)
            .filter_ok(|id| {
                if let Some(exclude) = self.exclude {
                    !exclude.utxos.contains(id)
                } else {
                    true
                }
            })
            .map(move |res| {
                res.map_err(StorageError::from).and_then(|id| {
                    let coin = CoinQueryContext(self.database).coin(&id)?;

                    Ok(Resource::Coin { id, fields: coin })
                })
            })
            .filter_ok(|coin| {
                if let Resource::Coin { fields, .. } = coin {
                    let is_unspent = fields.status == CoinStatus::Unspent;
                    self.assets
                        .as_ref()
                        .map(|assets| assets.contains(&fields.asset_id) && is_unspent)
                        .unwrap_or(is_unspent)
                } else {
                    true
                }
            });

        let messages_iter = MessageQueryContext(self.database)
            .owned_message_ids(self.owner, None, IterDirection::Forward)
            .filter_ok(|id| {
                if let Some(exclude) = self.exclude {
                    !exclude.messages.contains(id)
                } else {
                    true
                }
            })
            .map(move |res| {
                res.and_then(|id| {
                    let message = MessageQueryContext(self.database).message(&id)?;

                    Ok(Resource::Message {
                        id,
                        fields: message,
                    })
                })
            })
            .filter_ok(|message| {
                if let Resource::Message { fields, .. } = message {
                    fields.fuel_block_spend.is_none()
                } else {
                    true
                }
            });

        coins_iter.chain(messages_iter.take_while(|_| {
            self.assets
                .as_ref()
                .map(|assets| assets.contains(&AssetId::BASE))
                .unwrap_or(true)
        }))
    }
}

pub struct AssetQuery<'a> {
    pub owner: &'a Address,
    pub asset: &'a AssetSpendTarget,
    pub exclude: Option<&'a Exclude>,
    pub database: &'a DatabaseTemp,
    query: AssetsQuery<'a>,
}

impl<'a> AssetQuery<'a> {
    pub fn new(
        owner: &'a Address,
        asset: &'a AssetSpendTarget,
        exclude: Option<&'a Exclude>,
        database: &'a DatabaseTemp,
    ) -> Self {
        let mut allowed = HashSet::new();
        allowed.insert(&asset.id);
        Self {
            owner,
            asset,
            exclude,
            database,
            query: AssetsQuery::new(owner, Some(allowed), exclude, database),
        }
    }

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) resources of the `owner`
    /// for the `asset_id`.
    pub fn unspent_resources(
        &self,
    ) -> impl Iterator<Item = StorageResult<Resource<Coin, Message>>> + '_ {
        self.query.unspent_resources()
    }
}

/// The id of the resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceId {
    Utxo(UtxoId),
    Message(MessageId),
}

/// The primary type of spent or not spent resources(coins, messages, etc). The not spent resource
/// can be used as a source of information for the creation of the transaction's input.
#[derive(Debug)]
pub enum Resource<C, M> {
    Coin { id: UtxoId, fields: C },
    Message { id: MessageId, fields: M },
}

impl<C, M> Resource<C, M>
where
    C: Borrow<Coin>,
    M: Borrow<Message>,
{
    pub fn amount(&self) -> &Word {
        match self {
            Resource::Coin { fields, .. } => &fields.borrow().amount,
            Resource::Message { fields, .. } => &fields.borrow().amount,
        }
    }

    pub fn asset_id(&self) -> &AssetId {
        match self {
            Resource::Coin { fields, .. } => &fields.borrow().asset_id,
            Resource::Message { .. } => &AssetId::BASE,
        }
    }
}

impl<'c, 'm, C, M> Resource<Cow<'c, C>, Cow<'m, M>>
where
    C: Clone,
    M: Clone,
{
    /// Return owned `C` or `M`. Will clone if is borrowed.
    pub fn into_owned(self) -> Resource<C, M> {
        match self {
            Resource::Coin { id, fields } => Resource::Coin {
                id,
                fields: fields.into_owned(),
            },
            Resource::Message { id, fields } => Resource::Message {
                id,
                fields: fields.into_owned(),
            },
        }
    }
}
