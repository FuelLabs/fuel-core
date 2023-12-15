use serde::ser::*;

use crate::prelude::QueryExecutionError;
use thiserror::Error;

/// Error caused while processing a [Subscription](struct.Subscription.html) request.
#[derive(Debug, Error)]
pub enum SubscriptionError {
    #[error("GraphQL error: {0:?}")]
    GraphQLError(Vec<QueryExecutionError>),
}

impl From<QueryExecutionError> for SubscriptionError {
    fn from(e: QueryExecutionError) -> Self {
        SubscriptionError::GraphQLError(vec![e])
    }
}

impl From<Vec<QueryExecutionError>> for SubscriptionError {
    fn from(e: Vec<QueryExecutionError>) -> Self {
        SubscriptionError::GraphQLError(e)
    }
}
impl Serialize for SubscriptionError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        let msg = format!("{}", self);
        map.serialize_entry("message", msg.as_str())?;
        map.end()
    }
}
