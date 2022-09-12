use crate::database::Database;
use fuel_core_interfaces::{
    common::{
        fuel_storage::Storage,
        fuel_tx::UtxoId,
        fuel_types::{
            Address,
            AssetId,
            MessageId,
            Word,
        },
    },
    db::{
        Error,
        KvStoreError,
    },
    model::{
        Coin,
        CoinStatus,
        Message,
    },
};
use itertools::Itertools;
use std::{
    borrow::Cow,
    collections::HashSet,
};

const BASE_ASSET: AssetId = AssetId::zeroed();

/// At least required `target` of the query per asset's `id`.
#[derive(Clone)]
pub struct Asset {
    pub id: AssetId,
    pub target: u64,
    pub max: usize,
}

impl Asset {
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

pub struct AssetQuery<'a> {
    pub owner: &'a Address,
    pub asset: &'a Asset,
    pub exclude: Option<&'a Exclude>,
    pub database: &'a Database,
}

impl<'a> AssetQuery<'a> {
    pub fn new(
        owner: &'a Address,
        asset: &'a Asset,
        exclude: Option<&'a Exclude>,
        database: &'a Database,
    ) -> Self {
        Self {
            owner,
            asset,
            exclude,
            database,
        }
    }

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) inputs of the `owner`
    /// for the `asset_id`.
    // TODO: Optimize this by creating an index
    pub fn unspent_inputs(
        &self,
    ) -> impl Iterator<Item = Result<Resource<Cow<Coin>, Cow<Message>>, Error>> + '_ {
        let coins_iter = self
            .database
            .owned_coins_utxos(self.owner, None, None)
            .filter_ok(|id| {
                if let Some(excluder) = self.exclude {
                    !excluder.utxos.contains(id)
                } else {
                    true
                }
            })
            .map(|res| {
                res.map(|id| {
                    let coin = Storage::<UtxoId, Coin>::get(self.database, &id)?
                        .ok_or(KvStoreError::NotFound)?;

                    Ok::<_, KvStoreError>(Resource::Coin { id, fields: coin })
                })
            })
            .map(|results| Ok(results??))
            .filter_ok(|coin| {
                if let Resource::Coin { fields, .. } = coin {
                    fields.asset_id == self.asset.id
                        && fields.status == CoinStatus::Unspent
                } else {
                    true
                }
            });

        // TODO: If asset_id is zero, also check messages.

        coins_iter
    }
}

/// The id of the resource.
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

impl<C: AsRef<Coin>, M: AsRef<Message>> Resource<C, M> {
    pub fn amount(&self) -> &Word {
        match self {
            Resource::Coin { fields, .. } => &fields.as_ref().amount,
            Resource::Message { fields, .. } => &fields.as_ref().amount,
        }
    }

    pub fn asset_id(&self) -> &AssetId {
        match self {
            Resource::Coin { fields, .. } => &fields.as_ref().asset_id,
            Resource::Message { .. } => &BASE_ASSET,
        }
    }
}

impl<'c, 'm, C, M> Resource<Cow<'c, C>, Cow<'m, M>>
where
    C: Clone + AsRef<Coin>,
    M: Clone + AsRef<Message>,
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
