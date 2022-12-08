use crate::client::schema::{
    coin::Coin,
    message::Message,
    schema,
    Address,
    AssetId,
    ConversionError,
    MessageId,
    UtxoId,
    U64,
};
use itertools::Itertools;
use std::str::FromStr;

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ExcludeInput {
    /// Utxos to exclude from the result.
    utxos: Vec<UtxoId>,
    /// Messages to exclude from teh result.
    messages: Vec<MessageId>,
}

impl ExcludeInput {
    pub fn from_tuple(tuple: (Vec<&str>, Vec<&str>)) -> Result<Self, ConversionError> {
        let utxos = tuple.0.into_iter().map(UtxoId::from_str).try_collect()?;
        let messages = tuple.1.into_iter().map(MessageId::from_str).try_collect()?;

        Ok(Self { utxos, messages })
    }
}

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct SpendQueryElementInput {
    /// asset ID of the coins
    pub asset_id: AssetId,
    /// address of the owner
    pub amount: U64,
    /// the maximum number of resources per asset from the owner to return.
    pub max: Option<U64>,
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Resource {
    Coin(Coin),
    Message(Message),
    #[cynic(fallback)]
    Unknown,
}

impl Resource {
    pub fn amount(&self) -> u64 {
        match self {
            Resource::Coin(c) => c.amount.0,
            Resource::Message(m) => m.amount.0,
            Resource::Unknown => 0,
        }
    }
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ResourcesToSpendArgs {
    /// The `Address` of the assets' resources owner.
    owner: Address,
    /// The total amount of each asset type to spend
    query_per_asset: Vec<SpendQueryElementInput>,
    /// A list of ids to exclude from the selection
    excluded_ids: Option<ExcludeInput>,
}

pub(crate) type ResourcesToSpendArgsTuple =
    (Address, Vec<SpendQueryElementInput>, Option<ExcludeInput>);

impl From<ResourcesToSpendArgsTuple> for ResourcesToSpendArgs {
    fn from(r: ResourcesToSpendArgsTuple) -> Self {
        ResourcesToSpendArgs {
            owner: r.0,
            query_per_asset: r.1,
            excluded_ids: r.2,
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ResourcesToSpendArgs"
)]
pub struct ResourcesToSpendQuery {
    #[arguments(owner: $owner, queryPerAsset: $query_per_asset, excludedIds: $excluded_ids)]
    pub resources_to_spend: Vec<Vec<Resource>>,
}
