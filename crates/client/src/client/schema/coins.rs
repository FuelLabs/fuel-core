use crate::client::{
    schema::{
        schema,
        Address,
        AssetId,
        Nonce,
        PageInfo,
        UtxoId,
        U16,
        U32,
        U64,
    },
    PageDirection,
    PaginationRequest,
};

#[derive(cynic::QueryVariables, Debug)]
pub struct CoinByIdArgs {
    pub utxo_id: UtxoId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "CoinByIdArgs"
)]
pub struct CoinByIdQuery {
    #[arguments(utxoId: $ utxo_id)]
    pub coin: Option<Coin>,
}

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinFilterInput {
    /// Filter coins based on the `owner` field
    pub owner: Address,
    /// Filter coins based on the `asset_id` field
    pub asset_id: Option<AssetId>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct CoinsConnectionArgs {
    /// Filter coins based on a filter
    filter: CoinFilterInput,
    /// Skip until coin id (forward pagination)
    pub after: Option<String>,
    /// Skip until coin id (backward pagination)
    pub before: Option<String>,
    /// Retrieve the first n coins in order (forward pagination)
    pub first: Option<i32>,
    /// Retrieve the last n coins in order (backward pagination).
    /// Can't be used at the same time as `first`.
    pub last: Option<i32>,
}

impl From<(Address, AssetId, PaginationRequest<String>)> for CoinsConnectionArgs {
    fn from(r: (Address, AssetId, PaginationRequest<String>)) -> Self {
        match r.2.direction {
            PageDirection::Forward => CoinsConnectionArgs {
                filter: CoinFilterInput {
                    owner: r.0,
                    asset_id: Some(r.1),
                },
                after: r.2.cursor,
                before: None,
                first: Some(r.2.results),
                last: None,
            },
            PageDirection::Backward => CoinsConnectionArgs {
                filter: CoinFilterInput {
                    owner: r.0,
                    asset_id: Some(r.1),
                },
                after: None,
                before: r.2.cursor,
                first: None,
                last: Some(r.2.results),
            },
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "CoinsConnectionArgs"
)]
pub struct CoinsQuery {
    #[arguments(filter: $ filter, after: $ after, before: $ before, first: $ first, last: $ last)]
    pub coins: CoinConnection,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinConnection {
    pub edges: Vec<CoinEdge>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinEdge {
    pub cursor: String,
    pub node: Coin,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Coin {
    pub amount: U64,
    pub block_created: U32,
    pub tx_created_idx: U16,
    pub asset_id: AssetId,
    pub utxo_id: UtxoId,
    pub owner: Address,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Coin")]
pub struct CoinIdFragment {
    pub utxo_id: UtxoId,
}

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ExcludeInput {
    /// Utxos to exclude from the result.
    utxos: Vec<UtxoId>,
    /// Messages to exclude from the result.
    messages: Vec<Nonce>,
}

impl From<(Vec<UtxoId>, Vec<Nonce>)> for ExcludeInput {
    fn from(value: (Vec<UtxoId>, Vec<Nonce>)) -> Self {
        let (utxos, messages) = value;
        Self { utxos, messages }
    }
}

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct SpendQueryElementInput {
    /// asset ID of the coins
    pub asset_id: AssetId,
    /// address of the owner
    pub amount: U64,
    /// the maximum number of coins per asset from the owner to return.
    pub max: Option<U32>,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct MessageCoin {
    pub amount: U64,
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Nonce,
    pub da_height: U64,
}

#[derive(cynic::InlineFragments, Debug, Clone)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum CoinType {
    Coin(Coin),
    MessageCoin(MessageCoin),
    #[cynic(fallback)]
    Unknown,
}

impl CoinType {
    pub fn amount(&self) -> u64 {
        match self {
            CoinType::Coin(c) => c.amount.0,
            CoinType::MessageCoin(m) => m.amount.0,
            CoinType::Unknown => 0,
        }
    }
}

#[derive(cynic::QueryVariables, Debug)]
pub struct CoinsToSpendArgs {
    /// The `Address` of the assets' coins owner.
    owner: Address,
    /// The total amount of each asset type to spend.
    query_per_asset: Vec<SpendQueryElementInput>,
    /// A list of ids to exclude from the selection.
    excluded_ids: Option<ExcludeInput>,
}

pub(crate) type CoinsToSpendArgsTuple =
    (Address, Vec<SpendQueryElementInput>, Option<ExcludeInput>);

impl From<CoinsToSpendArgsTuple> for CoinsToSpendArgs {
    fn from(r: CoinsToSpendArgsTuple) -> Self {
        CoinsToSpendArgs {
            owner: r.0,
            query_per_asset: r.1,
            excluded_ids: r.2,
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "CoinsToSpendArgs"
)]
pub struct CoinsToSpendQuery {
    #[arguments(owner: $ owner, queryPerAsset: $ query_per_asset, excludedIds: $ excluded_ids)]
    pub coins_to_spend: Vec<Vec<CoinType>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coin_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = CoinByIdQuery::build(CoinByIdArgs {
            utxo_id: UtxoId::default(),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn coins_connection_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = CoinsQuery::build(CoinsConnectionArgs {
            filter: CoinFilterInput {
                owner: Address::default(),
                asset_id: Some(AssetId::default()),
            },
            after: None,
            before: None,
            first: None,
            last: None,
        });
        insta::assert_snapshot!(operation.query)
    }
}
