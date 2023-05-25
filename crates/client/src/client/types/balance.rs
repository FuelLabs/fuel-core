use crate::client::{
    schema,
    types::scalars::{
        Address,
        AssetId,
    },
    PaginatedResult,
};

pub struct Balance {
    pub owner: Address,
    pub amount: u64,
    pub asset_id: AssetId,
}

// GraphQL Translation

impl From<schema::balance::Balance> for Balance {
    fn from(value: schema::balance::Balance) -> Self {
        Balance {
            owner: value.owner.into(),
            amount: value.amount.into(),
            asset_id: value.asset_id.into(),
        }
    }
}

impl From<schema::balance::BalanceConnection> for PaginatedResult<Balance, String> {
    fn from(conn: schema::balance::BalanceConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node.into()).collect(),
        }
    }
}
