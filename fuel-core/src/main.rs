use fuel_core::service;

use actix_web::{App, HttpServer};
use structopt::StructOpt;
use tracing::{info, trace};

use std::io;

mod args;

#[actix_web::main]
async fn main() -> io::Result<()> {
    let config = args::Opt::from_args().exec()?;
    let addr = config.addr;

    trace!("Initializing in TRACE mode");
    info!("Binding GraphQL provider to {}", addr);
    HttpServer::new(move || App::new().configure(service::configure(config.clone())))
        .bind(addr)?
        .run()
        .await?;

    info!("Graceful shutdown");

    Ok(())
}
