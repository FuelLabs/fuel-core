use crate::schema::dap;

use actix_web::{guard, web};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.data(dap::schema())
        .service(web::resource("/dap").guard(guard::Post()).to(dap::service));
}
