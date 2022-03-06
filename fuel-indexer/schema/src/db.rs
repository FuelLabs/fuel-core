pub mod graphql;
pub mod models;
#[allow(unused_imports)]
pub mod schema;
pub mod tables;

pub use diesel::prelude::SqliteConnection;
pub use diesel::sqlite::Sqlite;
pub use diesel::prelude::PgConnection;
pub use diesel::pg::Pg;
