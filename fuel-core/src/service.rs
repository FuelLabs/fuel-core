use crate::schema::{dap, tx};

use crate::database::Database;
use actix_web::{guard, web};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default, Debug)]
pub struct SharedDatabase(pub Arc<Mutex<Database>>);

pub fn configure(cfg: &mut web::ServiceConfig) {
    let database = SharedDatabase::default();

    cfg.data(database);

    cfg.data(dap::schema())
        .service(web::resource("/dap").guard(guard::Post()).to(dap::service));

    cfg.data(tx::schema())
        .service(web::resource("/tx").guard(guard::Post()).to(tx::service));
}
