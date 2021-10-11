pub mod database;
pub mod schema;
pub mod service;
pub mod state;

use crate::database::{Database, SharedDatabase};
use service::{configure, DbType};
use std::io;
use std::sync::Arc;
use structopt::StructOpt;
use tracing::{info, trace};

mod args;

#[tokio::main]
async fn main() -> io::Result<()> {
    let config = args::Opt::from_args().exec()?;
    let addr = config.addr;

    let inner_database = match config.database_type {
        #[cfg(feature = "default")]
        DbType::RocksDb => Database::open(&config.database_path).expect("unable to open database"),
        DbType::InMemory => Database::default(),
        #[cfg(not(feature = "default"))]
        _ => Database::default(),
    };

    let database = SharedDatabase(Arc::new(inner_database));

    trace!("Initializing in TRACE mode");
    info!("Binding GraphQL provider to {}", addr);
    let app = configure(database);
    if let Err(err) = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        eprintln!("server error: {}", err);
    } else {
        info!("Graceful shutdown");
    }

    Ok(())
}
