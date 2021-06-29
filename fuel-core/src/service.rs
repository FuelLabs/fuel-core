use crate::schema::{dap, tx};

use actix_web::{guard, web};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.data(dap::schema())
        .service(web::resource("/dap").guard(guard::Post()).to(dap::service));

    cfg.data(tx::schema())
        .service(web::resource("/tx").guard(guard::Post()).to(tx::service));
}
