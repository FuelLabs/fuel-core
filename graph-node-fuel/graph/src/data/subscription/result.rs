use crate::prelude::QueryResult;
use std::pin::Pin;
use std::sync::Arc;

/// A stream of query results for a subscription.
pub type QueryResultStream =
    Pin<Box<dyn futures03::stream::Stream<Item = Arc<QueryResult>> + Send>>;

/// The result of running a subscription, if successful.
pub type SubscriptionResult = QueryResultStream;
