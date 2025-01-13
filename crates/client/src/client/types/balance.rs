use crate::client::{
    schema,
    types::primitives::{
        Address,
        AssetId,
    },
    PaginatedResult,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Balance {
    pub owner: Address,
    pub amount: u128,
    pub asset_id: AssetId,
}

// GraphQL Translation

impl From<schema::balance::Balance> for Balance {
    fn from(value: schema::balance::Balance) -> Self {
        Balance {
            owner: value.owner.into(),
            amount: {
                let amount: u64 = value.amount.into();
                u128::from(amount)
            },
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
