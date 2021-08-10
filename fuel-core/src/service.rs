use crate::schema::{dap, tx};

use crate::database::{Database, DatabaseTrait};
use actix_web::{guard, web};
use std::net;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: net::SocketAddr,
    pub database_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            addr: net::SocketAddr::new("127.0.0.1".parse().unwrap(), 0),
            database_path: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SharedDatabase(pub Arc<dyn DatabaseTrait + Send + Sync>);

pub fn configure(config: Config) -> impl Fn(&mut web::ServiceConfig) {
    let inner_database = if let Some(path) = config.database_path {
        Database::open(&path).expect("unable to open database")
    } else {
        Database::default()
    };

    let database = SharedDatabase(Arc::new(inner_database));
    move |cfg: &mut web::ServiceConfig| {
        cfg.data(dap::schema(Some(database.clone())))
            .service(web::resource("/dap").guard(guard::Post()).to(dap::service));

        cfg.data(tx::schema(Some(database.clone())))
            .service(web::resource("/tx").guard(guard::Post()).to(tx::service));
    }
}
