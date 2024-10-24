use std::future;

use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_services::yield_stream::StreamYieldExt;
use fuel_core_storage::{
    iter::IterDirection,
    Result as StorageResult,
};
use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
    },
    services::graphql_api::AddressBalance,
};
use futures::{
    stream,
    Stream,
    StreamExt,
    TryStreamExt,
};
use itertools::Either;
use tracing::debug;

pub mod asset_query;

impl ReadView {
    pub async fn balance(
        &self,
        owner: Address,
        asset_id: AssetId,
        base_asset_id: AssetId,
    ) -> StorageResult<AddressBalance> {
        let amount = self.off_chain.balance(&owner, &asset_id, &base_asset_id)?;

        Ok(AddressBalance {
            owner,
            amount,
            asset_id,
        })
    }

    pub fn balances<'a>(
        &'a self,
        owner: &'a Address,
        direction: IterDirection,
        base_asset_id: &'a AssetId,
    ) -> impl Stream<Item = StorageResult<AddressBalance>> + 'a {
        debug!("Querying balances for {:?}", owner);

        match self.off_chain.balances(owner, base_asset_id) {
            Ok(balances) => {
                let iter = if direction == IterDirection::Reverse {
                    Either::Left(balances.into_iter().rev())
                } else {
                    Either::Right(balances.into_iter())
                };
                stream::iter(iter.map(|(asset_id, amount)| AddressBalance {
                    owner: *owner,
                    amount,
                    asset_id,
                }))
                .map(Ok)
                .into_stream()
                .yield_each(self.batch_size)
                .left_stream()
            }
            Err(err) => stream::once(future::ready(Err(err))).right_stream(),
        }
    }
}
