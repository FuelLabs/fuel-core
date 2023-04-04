use async_graphql::Object;

#[derive(Default)]
pub struct HealthQuery;

#[Object]
impl HealthQuery {
    /// Returns true when the GraphQL API is serving requests.
    async fn health(&self) -> bool {
        true
    }
}
