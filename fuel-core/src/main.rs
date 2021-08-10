use fuel_core::database::Database;
use fuel_core::service;
use fuel_core::service::SharedDatabase;

use actix_web::{App, HttpServer};

use std::io;
use std::sync::Arc;
use structopt::StructOpt;
use tracing::{info, trace};

mod args;

#[actix_web::main]
async fn main() -> io::Result<()> {
    let config = args::Opt::from_args().exec()?;
    let addr = config.addr;

    let inner_database = if let Some(path) = config.database_path {
        Database::open(&path).expect("unable to open database")
    } else {
        Database::default()
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
