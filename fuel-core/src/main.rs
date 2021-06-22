use fuel_core::service;

use actix_web::{App, HttpServer};
use structopt::StructOpt;
use tracing::{info, trace};

use std::io;

mod args;

#[actix_web::main]
async fn main() -> io::Result<()> {
    let addr = args::Opt::from_args().exec()?;

    trace!("Initializing in TRACE mode");
    info!("Binding GraphQL provider to {}", addr);

    HttpServer::new(|| App::new().configure(service::configure))
        .bind(addr)?
        .run()
        .await?;

    info!("Graceful shutdown");

    Ok(())
}
