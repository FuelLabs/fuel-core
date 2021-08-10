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
