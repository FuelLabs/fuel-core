use crate::schema::{dap, tx};

use crate::database::{Database, DatabaseTrait};
use actix_web::{guard, web};
use fuel_vm::data::InterpreterStorage;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct SharedDatabase(pub Arc<dyn DatabaseTrait + Send + Sync>);

pub fn configure(cfg: &mut web::ServiceConfig) {
    let database = SharedDatabase(Arc::new(Database::default()));

    cfg.data(dap::schema(Some(database.clone())))
        .service(web::resource("/dap").guard(guard::Post()).to(dap::service));

    cfg.data(tx::schema(Some(database.clone())))
        .service(web::resource("/tx").guard(guard::Post()).to(tx::service));
}
