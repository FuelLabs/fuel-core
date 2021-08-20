use actix_web::{App, HttpServer};
use fuel_core::database::Database;
use fuel_core::service;
use fuel_core::service::{DbType, SharedDatabase};
use std::io;
use std::sync::Arc;
use structopt::StructOpt;
use tracing::{info, trace};

mod args;

#[actix_web::main]
async fn main() -> io::Result<()> {
    let config = args::Opt::from_args().exec()?;
    let addr = config.addr;

    let inner_database = match config.database_type {
        DbType::RocksDb => Database::open(&config.database_path).expect("unable to open database"),
        DbType::InMemory => Database::default(),
    };

    let database = SharedDatabase(Arc::new(inner_database));

    trace!("Initializing in TRACE mode");
    info!("Binding GraphQL provider to {}", addr);
    HttpServer::new(move || App::new().configure(service::configure(database.clone())))
        .bind(addr)?
        .run()
        .await?;

    info!("Graceful shutdown");

    Ok(())
}
