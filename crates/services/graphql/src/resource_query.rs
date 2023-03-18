use crate::{
    fuel_core_graphql_api::service::Database,
    query::asset_query::{
        AssetQuery,
        AssetSpendTarget,
        Exclude,
    },
};
use core::mem::swap;
use fuel_core_storage::Error as StorageError;
use fuel_core_types::{
    entities::resource::{
        Resource,
        ResourceId,
    },
    fuel_types::{
        Address,
        AssetId,
        Word,
    },
};
use itertools::Itertools;
use rand::prelude::*;
use std::{
    cmp::Reverse,
    collections::HashSet,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResourceQueryError {
    #[error("store error occurred")]
    StorageError(StorageError),
    #[error("not enough resources to fit the target")]
    InsufficientResources {
        asset_id: AssetId,
        collected_amount: Word,
    },
    #[error("max number of resources is reached while trying to fit the target")]
    MaxResourcesReached,
    #[error("the query contains duplicate assets")]
    DuplicateAssets(AssetId),
}

#[cfg(feature = "test-helpers")]
impl PartialEq for ResourceQueryError {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

/// The prepared spend queries.
pub struct SpendQuery {
    owner: Address,
    query_per_asset: Vec<AssetSpendTarget>,
    exclude: Exclude,
}

impl SpendQuery {
    // TODO: Check that number of `queries` is not too high(to prevent attacks).
    //  https://github.com/FuelLabs/fuel-core/issues/588#issuecomment-1240074551
    pub fn new(
        owner: Address,
        query_per_asset: &[AssetSpendTarget],
        exclude_vec: Option<Vec<ResourceId>>,
    ) -> Result<Self, ResourceQueryError> {
        let mut duplicate_checker = HashSet::new();

        for query in query_per_asset {
            if duplicate_checker.contains(&query.id) {
                return Err(ResourceQueryError::DuplicateAssets(query.id))
            }
            duplicate_checker.insert(query.id);
        }

        let exclude = if let Some(exclude_vec) = exclude_vec {
            Exclude::new(exclude_vec)
        } else {
            Default::default()
        };

        Ok(Self {
            owner,
            query_per_asset: query_per_asset.into(),
            exclude,
        })
    }

    /// Return [`Asset`]s.
    pub fn assets(&self) -> &Vec<AssetSpendTarget> {
        &self.query_per_asset
    }

    /// Return [`AssetQuery`]s.
    pub fn asset_queries<'a>(&'a self, db: &'a Database) -> Vec<AssetQuery<'a>> {
        self.query_per_asset
            .iter()
            .map(|asset| AssetQuery::new(&self.owner, asset, Some(&self.exclude), db))
            .collect()
    }

    /// Returns exclude that contains information about excluded ids.
    pub fn exclude(&self) -> &Exclude {
        &self.exclude
    }

    /// Returns the owner of the query.
    pub fn owner(&self) -> &Address {
        &self.owner
    }
}

/// Returns the biggest inputs of the `owner` to satisfy the required `target` of the asset. The
/// number of inputs for each asset can't exceed `max_inputs`, otherwise throw an error that query
/// can't be satisfied.
pub fn largest_first(query: &AssetQuery) -> Result<Vec<Resource>, ResourceQueryError> {
    let mut inputs: Vec<_> = query.resources().try_collect()?;
    inputs.sort_by_key(|resource| Reverse(*resource.amount()));

    let mut collected_amount = 0u64;
    let mut resources = vec![];

    for resource in inputs {
        // Break if we don't need any more coins
        if collected_amount >= query.asset.target {
            break
        }

        // Error if we can't fit more coins
        if resources.len() >= query.asset.max {
            return Err(ResourceQueryError::MaxResourcesReached)
        }

        // Add to list
        collected_amount = collected_amount.saturating_add(*resource.amount());
        resources.push(resource);
    }

    if collected_amount < query.asset.target {
        return Err(ResourceQueryError::InsufficientResources {
            asset_id: query.asset.id,
            collected_amount,
        })
    }

    Ok(resources)
}

// An implementation of the method described on: https://iohk.io/en/blog/posts/2018/07/03/self-organisation-in-coin-selection/
pub fn random_improve(
    db: &Database,
    spend_query: &SpendQuery,
) -> Result<Vec<Vec<Resource>>, ResourceQueryError> {
    let mut resources_per_asset = vec![];

    for query in spend_query.asset_queries(db) {
        let mut inputs: Vec<_> = query.resources().try_collect()?;
        inputs.shuffle(&mut thread_rng());
        inputs.truncate(query.asset.max);

        let mut collected_amount = 0;
        let mut resources = vec![];

        // Set parameters according to spec
        let target = query.asset.target;
        let upper_target = query.asset.target.saturating_mul(2);

        for resource in inputs {
            // Try to improve the result by adding dust to the result.
            if collected_amount >= target {
                // Break if found resource exceeds max `u64` or the upper limit
                if collected_amount == u64::MAX || resource.amount() > &upper_target {
                    break
                }

                // Break if adding doesn't improve the distance
                let change_amount = collected_amount - target;
                let distance = target.abs_diff(change_amount);
                let next_distance = target.abs_diff(change_amount + resource.amount());
                if next_distance >= distance {
                    break
                }
            }

            // Add to list
            collected_amount = collected_amount.saturating_add(*resource.amount());
            resources.push(resource);
        }

        // Fallback to largest_first if we can't fit more coins
        if collected_amount < query.asset.target {
            swap(&mut resources, &mut largest_first(&query)?);
        }

        resources_per_asset.push(resources);
    }

    Ok(resources_per_asset)
}

impl From<StorageError> for ResourceQueryError {
    fn from(e: StorageError) -> Self {
        ResourceQueryError::StorageError(e)
    }
}
