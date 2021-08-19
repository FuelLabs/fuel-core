use crate::schema::{dap, tx};

use crate::database::{Database, DatabaseTrait};
use actix_web::{guard, web};
use std::net;
use std::path::PathBuf;
use std::sync::Arc;
use strum_macros::Display;
use strum_macros::EnumString;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: net::SocketAddr,
    pub database_path: PathBuf,
    pub database_type: DbType,
}

#[derive(Clone, Debug, PartialEq, EnumString, Display)]
pub enum DbType {
    InMemory,
    RocksDb,
}

#[derive(Clone, Debug)]
pub struct SharedDatabase(pub Arc<dyn DatabaseTrait + Send + Sync>);

impl Default for SharedDatabase {
    fn default() -> Self {
        SharedDatabase(Arc::new(Database::default()))
    }
}

pub fn configure(db: SharedDatabase) -> impl Fn(&mut web::ServiceConfig) {
    move |cfg: &mut web::ServiceConfig| {
        cfg.data(dap::schema(Some(db.clone())))
            .service(web::resource("/dap").guard(guard::Post()).to(dap::service));

        cfg.data(tx::schema(Some(db.clone())))
            .service(web::resource("/tx").guard(guard::Post()).to(tx::service));
    }
}
