use diesel::PgConnection;
use std::{collections::HashMap, thread::sleep, time::Duration};

use graph::prelude::anyhow;
use graph_store_postgres::connection_pool::ConnectionPool;

use crate::manager::catalog;

pub fn run(pool: ConnectionPool, delay: u64) -> Result<(), anyhow::Error> {
    fn query(conn: &PgConnection) -> Result<Vec<(String, i64, i64)>, anyhow::Error> {
        use catalog::pg_catalog::pg_stat_database as d;
        use diesel::dsl::*;
        use diesel::sql_types::BigInt;
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};

        let rows = d::table
            .filter(d::datname.eq(any(vec!["explorer", "graph"])))
            .select((
                d::datname,
                sql::<BigInt>("(xact_commit + xact_rollback)::bigint"),
                sql::<BigInt>("txid_current()::bigint"),
            ))
            //.select((d::datname))
            .load::<(Option<String>, i64, i64)>(conn)?;
        Ok(rows
            .into_iter()
            .map(|(datname, all_txn, write_txn)| {
                (datname.unwrap_or("none".to_string()), all_txn, write_txn)
            })
            .collect())
    }

    let mut speeds = HashMap::new();
    let conn = pool.get()?;
    for (datname, all_txn, write_txn) in query(&conn)? {
        speeds.insert(datname, (all_txn, write_txn));
    }
    println!(
        "Looking for number of transactions performed in {}s ...",
        delay
    );
    sleep(Duration::from_secs(delay));
    println!("Number of transactions/minute");
    println!("{:10} {:>7} write", "database", "all");
    for (datname, all_txn, write_txn) in query(&conn)? {
        let (all_speed, write_speed) = speeds
            .get(&datname)
            .map(|(all_txn_old, write_txn_old)| {
                (all_txn - *all_txn_old, write_txn - *write_txn_old)
            })
            .unwrap_or((0, 0));
        let all_speed = all_speed as f64 * 60.0 / delay as f64;
        let write_speed = write_speed as f64 * 60.0 / delay as f64;
        println!("{:10} {:>7} {}", datname, all_speed, write_speed);
    }

    Ok(())
}
