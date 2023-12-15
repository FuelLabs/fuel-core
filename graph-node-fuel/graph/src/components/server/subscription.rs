use async_trait::async_trait;

/// Common trait for GraphQL subscription servers.
#[async_trait]
pub trait SubscriptionServer {
    /// Returns a Future that, when spawned, brings up the GraphQL subscription server.
    async fn serve(self, port: u16);
}
