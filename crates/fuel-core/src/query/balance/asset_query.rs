use crate::{
    graphql_api::database::ReadView,
    query::{
        CoinQueryData,
        MessageQueryData,
    },
};
use fuel_core_storage::{
    iter::IterDirection,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_types::{
    entities::coins::{
        CoinId,
        CoinType,
    },
    fuel_types::{
        Address,
        AssetId,
    },
};
use itertools::Itertools;
use std::collections::HashSet;

/// At least required `target` of the query per asset's `id` with `max` coins.
#[derive(Clone)]
pub struct AssetSpendTarget {
    pub id: AssetId,
    pub target: u64,
    pub max: usize,
}

impl AssetSpendTarget {
    pub fn new(id: AssetId, target: u64, max: usize) -> Self {
        Self { id, target, max }
    }
}

#[derive(Default)]
pub struct Exclude {
    pub coin_ids: HashSet<CoinId>,
}

impl Exclude {
    pub fn new(ids: Vec<CoinId>) -> Self {
        let mut instance = Self::default();

        for id in ids.into_iter() {
            instance.coin_ids.insert(id);
        }

        instance
    }
}

pub struct AssetsQuery<'a> {
    pub owner: &'a Address,
    pub assets: Option<HashSet<&'a AssetId>>,
    pub exclude: Option<&'a Exclude>,
    pub database: &'a ReadView,
    pub base_asset_id: &'a AssetId,
}

impl<'a> AssetsQuery<'a> {
    pub fn new(
        owner: &'a Address,
        assets: Option<HashSet<&'a AssetId>>,
        exclude: Option<&'a Exclude>,
        database: &'a ReadView,
        base_asset_id: &'a AssetId,
    ) -> Self {
        Self {
            owner,
            assets,
            exclude,
            database,
            base_asset_id,
        }
    }

    fn coins_iter(&self) -> impl Iterator<Item = StorageResult<CoinType>> + '_ {
        self.database
            .owned_coins_ids(self.owner, None, IterDirection::Forward)
            .map(|id| id.map(CoinId::from))
            .filter_ok(|id| {
                if let Some(exclude) = self.exclude {
                    !exclude.coin_ids.contains(id)
                } else {
                    true
                }
            })
            .map(move |res| {
                res.map_err(StorageError::from).and_then(|id| {
                    let id = if let CoinId::Utxo(id) = id {
                        id
                    } else {
                        return Err(anyhow::anyhow!("The coin is not UTXO").into());
                    };
                    let coin = self.database.coin(id)?;

                    Ok(CoinType::Coin(coin))
                })
            })
            .filter_ok(|coin| {
                if let CoinType::Coin(coin) = coin {
                    self.has_asset(&coin.asset_id)
                } else {
                    true
                }
            })
    }

    fn messages_iter(&self) -> impl Iterator<Item = StorageResult<CoinType>> + '_ {
        self.database
            .owned_message_ids(self.owner, None, IterDirection::Forward)
            .map(|id| id.map(CoinId::from))
            .filter_ok(|id| {
                if let Some(exclude) = self.exclude {
                    !exclude.coin_ids.contains(id)
                } else {
                    true
                }
            })
            .map(move |res| {
                res.and_then(|id| {
                    let id = if let CoinId::Message(id) = id {
                        id
                    } else {
                        return Err(anyhow::anyhow!("The coin is not a message").into());
                    };
                    let message = self.database.message(&id)?;
                    Ok(message)
                })
            })
            .filter_ok(|message| message.data().is_empty())
            .map(|result| {
                result.map(|message| {
                    CoinType::MessageCoin(
                        message
                            .try_into()
                            .expect("The checked above that message data is empty."),
                    )
                })
            })
    }

    fn has_asset(&self, asset_id: &AssetId) -> bool {
        self.assets
            .as_ref()
            .map(|assets| assets.contains(asset_id))
            .unwrap_or(true)
    }

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) coins of the `owner`.
    ///
    /// # Note: The coins of different type are not grouped by the `asset_id`.
    // TODO: Optimize this by creating an index
    //  https://github.com/FuelLabs/fuel-core/issues/588
    pub fn coins(&self) -> impl Iterator<Item = StorageResult<CoinType>> + '_ {
        let has_base_asset = self.has_asset(self.base_asset_id);
        let messages_iter = has_base_asset
            .then(|| self.messages_iter())
            .into_iter()
            .flatten();
        self.coins_iter().chain(messages_iter)
    }
}

pub struct AssetQuery<'a> {
    pub owner: &'a Address,
    pub asset: &'a AssetSpendTarget,
    pub exclude: Option<&'a Exclude>,
    pub database: &'a ReadView,
    query: AssetsQuery<'a>,
}

impl<'a> AssetQuery<'a> {
    pub fn new(
        owner: &'a Address,
        asset: &'a AssetSpendTarget,
        base_asset_id: &'a AssetId,
        exclude: Option<&'a Exclude>,
        database: &'a ReadView,
    ) -> Self {
        let mut allowed = HashSet::new();
        allowed.insert(&asset.id);
        Self {
            owner,
            asset,
            exclude,
            database,
            query: AssetsQuery::new(
                owner,
                Some(allowed),
                exclude,
                database,
                base_asset_id,
            ),
        }
    }

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) coins of the `owner`
    /// for the `asset_id`.
    pub fn coins(&self) -> impl Iterator<Item = StorageResult<CoinType>> + '_ {
        self.query.coins()
    }
}
