pub mod coins_query;
pub mod graphql_api;
pub mod query;
pub mod schema;

pub mod fuel_core_graphql_api {
    pub use crate::graphql_api::*;
}

pub use crate::graphql_api::*;

pub mod async_graphql {
    pub use async_graphql::*;
}
