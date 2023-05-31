use crate::client::{
    schema,
    types::primitives::{
        AssetId,
        Bytes,
        ContractId,
        Salt,
    },
    PaginatedResult,
};

pub struct Contract {
    pub id: ContractId,
    pub bytecode: Bytes,
    pub salt: Salt,
}

#[derive(Debug)]
pub struct ContractBalance {
    pub contract: ContractId,
    pub amount: u64,
    pub asset_id: AssetId,
}

// GraphQL Translation

impl From<schema::contract::Contract> for Contract {
    fn from(value: schema::contract::Contract) -> Self {
        Self {
            id: value.id.into(),
            bytecode: value.bytecode.into(),
            salt: value.salt.into(),
        }
    }
}

impl From<schema::contract::ContractBalance> for ContractBalance {
    fn from(value: schema::contract::ContractBalance) -> Self {
        Self {
            contract: value.contract.into(),
            amount: value.amount.into(),
            asset_id: value.asset_id.into(),
        }
    }
}

impl From<schema::contract::ContractBalanceConnection>
    for PaginatedResult<ContractBalance, String>
{
    fn from(conn: schema::contract::ContractBalanceConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node.into()).collect(),
        }
    }
}
