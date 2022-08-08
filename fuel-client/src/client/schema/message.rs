use crate::client::schema::{schema, Address, ConnectionArgs, U64};

use super::{PageInfo, PaginatedResult};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct DaMessage {
    pub amount: U64,
    pub sender: Address,
    pub recipient: Address,
    pub owner: Address,
    pub nonce: U64,
    pub data: Vec<i32>,
    pub da_height: U64,
    pub fuel_block_spend: Option<U64>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "ConnectionArgs"
)]
pub struct DaMessageQuery {
    #[arguments(after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub messages: DaMessageConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct DaMessageConnection {
    pub edges: Vec<DaMessageEdge>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct DaMessageEdge {
    pub cursor: String,
    pub node: DaMessage,
}

impl From<DaMessageConnection> for PaginatedResult<DaMessage, String> {
    fn from(conn: DaMessageConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn da_message_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = DaMessageQuery::build(ConnectionArgs::default());
        insta::assert_snapshot!(operation.query)
    }
}
