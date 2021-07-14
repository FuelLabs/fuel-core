use crate::schema::{dap, tx};

use crate::database::{Config, Database, MemoryDatabaseConfig};
use actix_web::{guard, web};

#[derive(Clone, Debug, Default)]
pub struct FuelServiceConfig<T: Config> {
    pub database: Database<T>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    let config = FuelServiceConfig {
        database: Database::<MemoryDatabaseConfig>::default(),
    };

    cfg.data(dap::stateful_schema(&config))
        .service(web::resource("/dap").guard(guard::Post()).to(dap::service));

    cfg.data(tx::stateful_schema(&config))
        .service(web::resource("/tx").guard(guard::Post()).to(tx::service));
}
