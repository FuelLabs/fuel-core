use crate::graphql_api::database::ReadView;
use fuel_core_services::stream::IntoBoxStream;
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
use futures::{
    Stream,
    TryStreamExt,
};
use std::collections::HashSet;
use tokio_stream::StreamExt;

/// At least required `target` of the query per asset's `id` with `max` coins.
#[derive(Clone)]
pub struct AssetSpendTarget {
    pub id: AssetId,
    pub target: u64,
    pub max: u16,
}

impl AssetSpendTarget {
    pub fn new(id: AssetId, target: u64, max: u16) -> Self {
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

#[derive(Clone)]
pub struct AssetsQuery<'a> {
    pub owner: &'a Address,
    pub allowed_assets: Option<HashSet<&'a AssetId>>,
    pub exclude: Option<&'a Exclude>,
    pub database: &'a ReadView,
    pub base_asset_id: &'a AssetId,
}

impl<'a> AssetsQuery<'a> {
    pub fn new(
        owner: &'a Address,
        allowed_assets: Option<HashSet<&'a AssetId>>,
        exclude: Option<&'a Exclude>,
        database: &'a ReadView,
        base_asset_id: &'a AssetId,
    ) -> Self {
        Self {
            owner,
            allowed_assets,
            exclude,
            database,
            base_asset_id,
        }
    }

    fn coins_iter(mut self) -> impl Stream<Item = StorageResult<CoinType>> + 'a {
        let allowed_assets = self.allowed_assets.take();
        let database = self.database;
        let stream = self
            .database
            .owned_coins_ids(self.owner, None, IterDirection::Forward)
            .map(|id| id.map(CoinId::from))
            .filter(move |result| {
                if let Ok(id) = result {
                    if let Some(exclude) = self.exclude {
                        !exclude.coin_ids.contains(id)
                    } else {
                        true
                    }
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
                    Ok(id)
                })
            });

        futures::stream::StreamExt::chunks(stream, database.batch_size)
            .map(|chunk| {
                use itertools::Itertools;

                let chunk = chunk.into_iter().try_collect::<_, Vec<_>, _>()?;
                Ok::<_, StorageError>(chunk)
            })
            .try_filter_map(move |chunk| async move {
                let chunk = database
                    .coins(chunk)
                    .await
                    .map(|result| result.map(CoinType::Coin));
                Ok(Some(futures::stream::iter(chunk)))
            })
            .try_flatten()
            .filter(move |result| {
                if let Ok(CoinType::Coin(coin)) = result {
                    allowed_asset(&allowed_assets, &coin.asset_id)
                } else {
                    true
                }
            })
    }

    fn messages_iter(&self) -> impl Stream<Item = StorageResult<CoinType>> + 'a {
        let exclude = self.exclude;
        let database = self.database;
        let stream = self
            .database
            .owned_message_ids(self.owner, None, IterDirection::Forward)
            .map(|id| id.map(CoinId::from))
            .filter(move |result| {
                if let Ok(id) = result {
                    if let Some(e) = exclude {
                        !e.coin_ids.contains(id)
                    } else {
                        true
                    }
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
                    Ok(id)
                })
            });

        futures::stream::StreamExt::chunks(stream, database.batch_size)
            .map(|chunk| {
                use itertools::Itertools;

                let chunk = chunk.into_iter().try_collect::<_, Vec<_>, _>()?;
                Ok(chunk)
            })
            .try_filter_map(move |chunk| async move {
                let chunk = database.messages(chunk).await;
                Ok::<_, StorageError>(Some(futures::stream::iter(chunk)))
            })
            .try_flatten()
            .filter(|result| {
                if let Ok(message) = result {
                    message.data().is_empty()
                } else {
                    true
                }
            })
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

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) coins of the `owner`.
    ///
    /// # Note: The coins of different type are not grouped by the `asset_id`.
    // TODO: Optimize this by creating an index
    //  https://github.com/FuelLabs/fuel-core/issues/588
    pub fn coins(self) -> impl Stream<Item = StorageResult<CoinType>> + 'a {
        let has_base_asset = allowed_asset(&self.allowed_assets, self.base_asset_id);
        if has_base_asset {
            let message_iter = self.messages_iter();
            self.coins_iter().chain(message_iter).into_boxed_ref()
        } else {
            self.coins_iter().into_boxed_ref()
        }
    }
}

#[derive(Clone)]
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
    pub fn coins(self) -> impl Stream<Item = StorageResult<CoinType>> + 'a {
        self.query.coins()
    }
}

fn allowed_asset(allowed_assets: &Option<HashSet<&AssetId>>, asset_id: &AssetId) -> bool {
    allowed_assets
        .as_ref()
        .map(|allowed_assets| allowed_assets.contains(asset_id))
        .unwrap_or(true)
}
