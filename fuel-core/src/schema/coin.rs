use crate::{
    coin_query::{
        random_improve,
        SpendQueryElement,
    },
    database::{
        Database,
        KvStoreError,
    },
    schema::scalars::{
        Address,
        AssetId,
        UtxoId,
        U64,
    },
    service::Config,
    state::IterDirection,
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        query,
        Connection,
        Edge,
        EmptyFields,
    },
    Context,
    Enum,
    InputObject,
    Object,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::Storage,
        fuel_tx,
    },
    model::{
        Coin as CoinModel,
        CoinStatus as CoinStatusModel,
    },
};
use itertools::Itertools;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(remote = "CoinStatusModel")]
pub enum CoinStatus {
    Unspent,
    Spent,
}

pub struct Coin(fuel_tx::UtxoId, CoinModel);

#[Object]
impl Coin {
    async fn utxo_id(&self) -> UtxoId {
        self.0.into()
    }

    async fn owner(&self) -> Address {
        self.1.owner.into()
    }

    async fn amount(&self) -> U64 {
        self.1.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.1.asset_id.into()
    }

    async fn maturity(&self) -> U64 {
        self.1.maturity.into()
    }

    async fn status(&self) -> CoinStatus {
        self.1.status.into()
    }

    async fn block_created(&self) -> U64 {
        self.1.block_created.into()
    }
}

#[derive(InputObject)]
struct CoinFilterInput {
    /// Address of the owner
    owner: Address,
    /// Asset ID of the coins
    asset_id: Option<AssetId>,
}

#[derive(InputObject)]
struct SpendQueryElementInput {
    /// Asset ID of the coins
    asset_id: AssetId,
    /// Target amount for the query
    amount: U64,
}

#[derive(Default)]
pub struct CoinQuery;

#[Object]
impl CoinQuery {
    async fn coin(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The ID of the coin")] utxo_id: UtxoId,
    ) -> async_graphql::Result<Option<Coin>> {
        let utxo_id = utxo_id.0;
        let db = ctx.data_unchecked::<Database>().clone();
        let block = Storage::<fuel_tx::UtxoId, CoinModel>::get(&db, &utxo_id)?
            .map(|coin| Coin(utxo_id, coin.into_owned()));
        Ok(block)
    }

    async fn coins(
        &self,
        ctx: &Context<'_>,
        filter: CoinFilterInput,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<UtxoId, Coin, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>();

        query(
            after,
            before,
            first,
            last,
            |after: Option<UtxoId>, before: Option<UtxoId>, first, last| {
                async move {
                    let (records_to_fetch, direction) = if let Some(first) = first {
                        (first, IterDirection::Forward)
                    } else if let Some(last) = last {
                        (last, IterDirection::Reverse)
                    } else {
                        (0, IterDirection::Forward)
                    };

                    if (first.is_some() && before.is_some())
                        || (after.is_some() && before.is_some())
                        || (last.is_some() && after.is_some())
                    {
                        return Err(anyhow!("Wrong argument combination"))
                    }

                    let after = after.map(fuel_tx::UtxoId::from);
                    let before = before.map(fuel_tx::UtxoId::from);

                    let start;
                    let end;

                    if direction == IterDirection::Forward {
                        start = after;
                        end = before;
                    } else {
                        start = before;
                        end = after;
                    }

                    let owner: fuel_tx::Address = filter.owner.into();

                    let mut coin_ids = db.owned_coins(owner, start, Some(direction));
                    let mut started = None;
                    if start.is_some() {
                        // skip initial result
                        started = coin_ids.next();
                    }

                    // take desired amount of results
                    let coins = coin_ids
                        .take_while(|r| {
                            // take until we've reached the end
                            if let (Ok(t), Some(end)) = (r, end.as_ref()) {
                                if *t == *end {
                                    return false
                                }
                            }
                            true
                        })
                        .take(records_to_fetch);
                    let mut coins: Vec<fuel_tx::UtxoId> = coins.try_collect()?;
                    if direction == IterDirection::Reverse {
                        coins.reverse();
                    }

                    // TODO: do a batch get instead
                    let coins: Vec<Coin> = coins
                        .into_iter()
                        .map(|id| {
                            Storage::<fuel_tx::UtxoId, CoinModel>::get(db, &id)
                                .transpose()
                                .ok_or(KvStoreError::NotFound)?
                                .map(|coin| Coin(id, coin.into_owned()))
                        })
                        .try_collect()?;

                    // filter coins by asset ID
                    let mut coins = coins;
                    if let Some(asset_id) = filter.asset_id {
                        coins.retain(|coin| coin.1.asset_id == asset_id.0);
                    }

                    // filter coins by status
                    coins.retain(|coin| coin.1.status == CoinStatusModel::Unspent);

                    let mut connection = Connection::new(
                        started.is_some(),
                        records_to_fetch <= coins.len(),
                    );
                    connection.edges.extend(
                        coins
                            .into_iter()
                            .map(|item| Edge::new(UtxoId::from(item.0), item)),
                    );

                    Ok::<Connection<UtxoId, Coin>, anyhow::Error>(connection)
                }
            },
        )
        .await
    }

    /// For each `spend_query`, get some spendable coins (of asset specified by the query) owned by
    /// `owner` that add up at least the query amount. The returned coins (UTXOs) are actual coins
    /// that can be spent. The number of coins (UXTOs) is optimized to prevent dust accumulation.
    /// Max number of UTXOS and excluded UTXOS can also be specified.
    async fn coins_to_spend(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The Address of the utxo owner")] owner: Address,
        #[graphql(desc = "The total amount of each asset type to spend")]
        spend_query: Vec<SpendQueryElementInput>,
        #[graphql(desc = "The max number of utxos that can be used")] max_inputs: Option<
            u64,
        >,
        #[graphql(desc = "The utxos that cannot be used")] excluded_ids: Option<
            Vec<UtxoId>,
        >,
    ) -> async_graphql::Result<Vec<Coin>> {
        let config = ctx.data_unchecked::<Config>();

        let owner: fuel_tx::Address = owner.0;
        let spend_query: Vec<SpendQueryElement> = spend_query
            .iter()
            .map(|e| (owner, e.asset_id.0, e.amount.0))
            .collect();
        let max_inputs: u64 =
            max_inputs.unwrap_or(config.chain_conf.transaction_parameters.max_inputs);
        let excluded_ids: Option<Vec<fuel_tx::UtxoId>> =
            excluded_ids.map(|ids| ids.into_iter().map(|id| id.0).collect());

        let db = ctx.data_unchecked::<Database>();

        let coins = random_improve(db, &spend_query, max_inputs, excluded_ids.as_ref())?
            .into_iter()
            .map(|(id, coin)| Coin(id, coin))
            .collect();

        Ok(coins)
    }
}
