use crate::database::Database;
use service::{configure, DbType};
use std::io;
use structopt::StructOpt;
use tracing::{info, trace};

mod args;
pub mod database;
pub(crate) mod executor;
pub mod model;
pub mod schema;
pub mod service;
pub mod state;
pub(crate) mod tx_pool;

#[tokio::main]
async fn main() -> io::Result<()> {
    let config = args::Opt::from_args().exec()?;
    let addr = config.addr;

    let database = match config.database_type {
        #[cfg(feature = "default")]
        DbType::RocksDb => Database::open(&config.database_path).expect("unable to open database"),
        DbType::InMemory => Database::default(),
        #[cfg(not(feature = "default"))]
        _ => Database::default(),
    };

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
