use crate::{
    fuel_core_graphql_api::{
        service::Database,
        Config as GraphQLConfig,
    },
    query::asset_query::AssetSpendTarget,
    resource_query::{
        random_improve,
        SpendQuery,
    },
    schema::{
        coin::Coin,
        message::Message,
        scalars::{
            message_id::MessageId,
            Address,
            AssetId,
            UtxoId,
            U64,
        },
    },
};
use async_graphql::{
    Context,
    InputObject,
    Object,
    Union,
};
use fuel_core_types::{
    entities::resource,
    fuel_tx,
};
use itertools::Itertools;

#[derive(InputObject)]
pub struct SpendQueryElementInput {
    /// Identifier of the asset to spend.
    asset_id: AssetId,
    /// Target amount for the query.
    amount: U64,
    /// The maximum number of currencies for selection.
    max: Option<U64>,
}

#[derive(InputObject)]
pub struct ExcludeInput {
    /// Utxos to exclude from the selection.
    utxos: Vec<UtxoId>,
    /// Messages to exclude from the selection.
    messages: Vec<MessageId>,
}

/// The schema analog of the [`resource::Resource`].
#[derive(Union)]
pub enum Resource {
    Coin(Coin),
    Message(Message),
}

#[derive(Default)]
pub struct ResourceQuery;

#[Object]
impl ResourceQuery {
    /// For each `query_per_asset`, get some spendable resources(of asset specified by the query) owned by
    /// `owner` that add up at least the query amount. The returned resources are actual resources
    /// that can be spent. The number of resources is optimized to prevent dust accumulation.
    /// Max number of resources and excluded resources can also be specified.
    ///
    /// Returns:
    ///     The list of spendable resources per asset from the query. The length of the result is
    ///     the same as the length of `query_per_asset`. The ordering of assets and `query_per_asset`
    ///     is the same.
    async fn resources_to_spend(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The `Address` of the resources owner.")] owner: Address,
        #[graphql(desc = "\
            The list of requested assets` resources with asset ids, `target` amount the user wants \
            to reach, and the `max` number of resources in the selection. Several entries with the \
            same asset id are not allowed.")]
        query_per_asset: Vec<SpendQueryElementInput>,
        #[graphql(desc = "The excluded resources from the selection.")]
        excluded_ids: Option<ExcludeInput>,
    ) -> async_graphql::Result<Vec<Vec<Resource>>> {
        let config = ctx.data_unchecked::<GraphQLConfig>();

        let owner: fuel_tx::Address = owner.0;
        let query_per_asset = query_per_asset
            .into_iter()
            .map(|e| {
                AssetSpendTarget::new(
                    e.asset_id.0,
                    e.amount.0,
                    e.max
                        .map(|max| max.0)
                        .unwrap_or(config.transaction_parameters.max_inputs),
                )
            })
            .collect_vec();
        let excluded_ids: Option<Vec<_>> = excluded_ids.map(|exclude| {
            let utxos = exclude
                .utxos
                .into_iter()
                .map(|utxo| resource::ResourceId::Utxo(utxo.0));
            let messages = exclude
                .messages
                .into_iter()
                .map(|message| resource::ResourceId::Message(message.0));
            utxos.chain(messages).collect()
        });

        let spend_query = SpendQuery::new(owner, &query_per_asset, excluded_ids)?;

        let db = ctx.data_unchecked::<Database>();

        let resources = random_improve(db, &spend_query)?
            .into_iter()
            .map(|resources| {
                resources
                    .into_iter()
                    .map(|resource| match resource {
                        resource::Resource::Coin(coin) => Resource::Coin(coin.into()),
                        resource::Resource::Message(message) => {
                            Resource::Message(message.into())
                        }
                    })
                    .collect_vec()
            })
            .collect();

        Ok(resources)
    }
}
