pub mod models;
pub mod tables;


cfg_if::cfg_if! {
    if #[cfg(feature = "diesel-sqlite")] {
        pub mod sqlite_schema;
        pub use diesel::prelude::SqliteConnection as Conn;
        pub use diesel::sqlite::Sqlite as DBBackend;
    } else if #[cfg(feature = "diesel-postgres")] {
        pub mod pg_schema;
        pub use diesel::prelude::PgConnection as Conn;
        pub use diesel::pg::Pg as DBBackend;
    }
}
