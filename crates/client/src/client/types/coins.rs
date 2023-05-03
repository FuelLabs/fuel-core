use crate::client::{
    schema,
    types::scalars::{
        Address,
        AssetId,
        UtxoId,
    },
    PaginatedResult,
};

pub struct Coin {
    pub amount: u64,
    pub block_created: u32,
    pub asset_id: AssetId,
    pub utxo_id: UtxoId,
    pub maturity: u32,
    pub owner: Address,
}

impl From<schema::coins::Coin> for Coin {
    fn from(value: schema::coins::Coin) -> Self {
        Self {
            amount: value.amount.into(),
            block_created: value.block_created.into(),
            asset_id: value.asset_id.into(),
            utxo_id: value.utxo_id.into(),
            maturity: value.maturity.into(),
            owner: value.owner.into(),
        }
    }
}

impl From<schema::coins::CoinConnection> for PaginatedResult<Coin, String> {
    fn from(conn: schema::coins::CoinConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node.into()).collect(),
        }
    }
}
