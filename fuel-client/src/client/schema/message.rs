use super::{
    PageDirection,
    PageInfo,
    PaginatedResult,
    PaginationRequest,
};
use crate::client::schema::{
    schema,
    Address,
    MessageId,
    U64,
};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Message {
    pub message_id: MessageId,
    pub amount: U64,
    pub sender: Address,
    pub recipient: Address,
    pub nonce: U64,
    pub data: Vec<i32>,
    pub da_height: U64,
    pub fuel_block_spend: Option<U64>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "OwnedMessagesConnectionArgs"
)]
pub struct OwnedMessageQuery {
    #[arguments(owner = &args.owner, after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub messages: MessageConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct MessageConnection {
    pub edges: Vec<MessageEdge>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct MessageEdge {
    pub cursor: String,
    pub node: Message,
}

#[derive(cynic::FragmentArguments, Debug)]
pub struct OwnedMessagesConnectionArgs {
    /// Filter messages based on an owner
    pub owner: Option<Address>,
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

impl From<(Option<Address>, PaginationRequest<String>)> for OwnedMessagesConnectionArgs {
    fn from(r: (Option<Address>, PaginationRequest<String>)) -> Self {
        match r.1.direction {
            PageDirection::Forward => OwnedMessagesConnectionArgs {
                owner: r.0,
                after: r.1.cursor,
                before: None,
                first: Some(r.1.results as i32),
                last: None,
            },
            PageDirection::Backward => OwnedMessagesConnectionArgs {
                owner: r.0,
                after: None,
                before: r.1.cursor,
                first: None,
                last: Some(r.1.results as i32),
            },
        }
    }
}

impl From<MessageConnection> for PaginatedResult<Message, String> {
    fn from(conn: MessageConnection) -> Self {
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
    fn owned_message_query_gql_output() {
        use cynic::QueryBuilder;

        let operation = OwnedMessageQuery::build(OwnedMessagesConnectionArgs {
            owner: Some(Address::default()),
            after: None,
            before: None,
            first: None,
            last: None,
        });

        insta::assert_snapshot!(operation.query)
    }
}
