use crate::coin_query::SpendQuery;
use crate::database::utils::Asset;
use crate::schema::coin::Coin;
use crate::schema::message::Message;
use crate::schema::scalars::message_id::MessageId;
use crate::schema::scalars::{AssetId, U64};
use crate::{
    coin_query::random_improve,
    database::Database,
    schema::scalars::{Address, UtxoId},
    service::Config,
};
use async_graphql::{Context, InputObject, Object, Union};
use fuel_core_interfaces::common::fuel_tx;
use itertools::Itertools;

#[derive(InputObject)]
pub struct SpendQueryElementInput {
    /// Asset ID of the coins
    asset_id: AssetId,
    /// Target amount for the query
    amount: U64,
}

#[derive(InputObject)]
pub struct ExcludeInput {
    /// Utxos to exclude from the selection.
    utxos: Vec<UtxoId>,
    /// Messages to exclude from teh selection.
    messages: Vec<MessageId>,
}

/// The schema analog of the [`crate::database::utils::Banknote`].
#[derive(Union)]
pub enum Banknote {
    Coin(Coin),
    Message(Message),
}

#[derive(Default)]
pub struct BanknoteQuery;

#[Object]
impl BanknoteQuery {
    /// For each `spend_query`, get some spendable banknotes(of asset specified by the query) owned by
    /// `owner` that add up at least the query amount. The returned banknotes are actual coins/messages
    /// that can be spent. The number of banknotes is optimized to prevent dust accumulation.
    /// Max number of banknotes and excluded banknotes can also be specified.
    // TODO: `max_inputs` should be per asset id.
    async fn banknotes_to_spend(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The Address of the utxo owner")] owner: Address,
        #[graphql(desc = "The total amount of each asset type to spend")] assets: Vec<
            SpendQueryElementInput,
        >,
        #[graphql(desc = "The max number of utxos that can be used")] max_inputs: Option<u64>,
        #[graphql(desc = "The utxos that cannot be used")] excluded_ids: Option<ExcludeInput>,
    ) -> async_graphql::Result<Vec<Vec<Banknote>>> {
        let config = ctx.data_unchecked::<Config>();

        let owner: fuel_tx::Address = owner.0;
        let assets = assets
            .into_iter()
            .map(|e| Asset::new(e.asset_id.0, e.amount.0))
            .collect_vec();
        let excluded_ids: Option<Vec<_>> = excluded_ids.map(|exclude| {
            let utxos = exclude
                .utxos
                .into_iter()
                .map(|utxo| crate::database::utils::BanknoteId::Utxo(utxo.0));
            let messages = exclude
                .messages
                .into_iter()
                .map(|message| crate::database::utils::BanknoteId::Message(message.0));
            utxos.chain(messages).collect()
        });

        let max_inputs: u64 =
            max_inputs.unwrap_or(config.chain_conf.transaction_parameters.max_inputs);

        let spend_query = SpendQuery::new(owner, &assets, excluded_ids, Some(max_inputs));

        let db = ctx.data_unchecked::<Database>();

        let banknotes = random_improve(db, &spend_query)?
            .into_iter()
            .map(|banknotes| {
                banknotes
                    .into_iter()
                    .map(|banknote| match banknote {
                        crate::database::utils::Banknote::Coin { id, fields } => {
                            Banknote::Coin(Coin(id, fields))
                        }
                        crate::database::utils::Banknote::Message { fields, .. } => {
                            Banknote::Message(Message(fields))
                        }
                    })
                    .collect_vec()
            })
            .collect();

        Ok(banknotes)
    }
}
