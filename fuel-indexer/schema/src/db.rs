pub mod graphql;
pub mod models;
#[allow(unused_imports)]
pub mod schema;
pub mod tables;


cfg_if::cfg_if! {
    if #[cfg(feature = "db-sqlite")] {
        pub use diesel::prelude::SqliteConnection as Conn;
        pub use diesel::sqlite::Sqlite as DBBackend;
    } else if #[cfg(feature = "db-postgres")] {
        pub use diesel::prelude::PgConnection as Conn;
        pub use diesel::pg::Pg DBBackend;
    }
}
