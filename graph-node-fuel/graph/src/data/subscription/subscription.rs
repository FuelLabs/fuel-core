use crate::prelude::Query;

/// A GraphQL subscription made by a client.
///
/// At the moment, this only contains the GraphQL query submitted as the
/// subscription payload.
#[derive(Clone, Debug)]
pub struct Subscription {
    /// The GraphQL subscription query.
    pub query: Query,
}
