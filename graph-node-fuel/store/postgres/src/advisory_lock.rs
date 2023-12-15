//! Helpers for using advisory locks. We centralize all use of these locks
//! in this module to make it easier to see what they are used for.
//!
//! The [Postgres
//! documentation](https://www.postgresql.org/docs/current/explicit-locking.html#ADVISORY-LOCKS)
//! has more details on advisory locks.
//!
//! We use the following 64 bit locks:
//!   * 1,2: to synchronize on migratons
//!
//! We use the following 2x 32-bit locks
//!   * 1, n: to lock copying of the deployment with id n in the destination
//!           shard
//!   * 2, n: to lock the deployment with id n to make sure only one write
//!           happens to it

use diesel::sql_types::Bool;
use diesel::{sql_query, PgConnection, RunQueryDsl};
use graph::prelude::StoreError;

use crate::command_support::catalog::Site;
use crate::primary::DeploymentId;

/// A locking scope for a particular deployment. We use different scopes for
/// different purposes, and in each scope we use an advisory lock for each
/// deployment.
struct Scope {
    id: i32,
}

impl Scope {
    /// Try to lock the deployment in this scope with the given id. Return
    /// `true` if we got the lock, and `false` if it is already locked.
    fn try_lock(&self, conn: &PgConnection, id: DeploymentId) -> Result<bool, StoreError> {
        #[derive(QueryableByName)]
        struct Locked {
            #[sql_type = "Bool"]
            locked: bool,
        }

        sql_query(format!(
            "select pg_try_advisory_lock({}, {id}) as locked",
            self.id
        ))
        .get_result::<Locked>(conn)
        .map(|res| res.locked)
        .map_err(StoreError::from)
    }

    /// Lock the deployment in this scope with the given id. Blocks until we
    /// can get the lock
    fn lock(&self, conn: &PgConnection, id: DeploymentId) -> Result<(), StoreError> {
        sql_query(format!("select pg_advisory_lock({}, {id})", self.id))
            .execute(conn)
            .map(|_| ())
            .map_err(StoreError::from)
    }

    /// Unlock the deployment in this scope with the given id.
    fn unlock(&self, conn: &PgConnection, id: DeploymentId) -> Result<(), StoreError> {
        sql_query(format!("select pg_advisory_unlock({}, {id})", self.id))
            .execute(conn)
            .map(|_| ())
            .map_err(StoreError::from)
    }
}

const COPY: Scope = Scope { id: 1 };
const WRITE: Scope = Scope { id: 2 };
const PRUNE: Scope = Scope { id: 3 };

/// Get a lock for running migrations. Blocks until we get the lock.
pub(crate) fn lock_migration(conn: &PgConnection) -> Result<(), StoreError> {
    sql_query("select pg_advisory_lock(1)").execute(conn)?;

    Ok(())
}

/// Release the migration lock.
pub(crate) fn unlock_migration(conn: &PgConnection) -> Result<(), StoreError> {
    sql_query("select pg_advisory_unlock(1)").execute(conn)?;
    Ok(())
}

/// Take the lock used to keep two copy operations to run simultaneously on
/// the same deployment. Block until we can get the lock
pub(crate) fn lock_copying(conn: &PgConnection, dst: &Site) -> Result<(), StoreError> {
    COPY.lock(conn, dst.id)
}

/// Release the lock acquired with `lock_copying`.
pub(crate) fn unlock_copying(conn: &PgConnection, dst: &Site) -> Result<(), StoreError> {
    COPY.unlock(conn, dst.id)
}

/// Take the lock used to keep two operations from writing to the deployment
/// simultaneously. Return `true` if we got the lock, and `false` if we did
/// not. You don't want to use this directly. Instead, use
/// `deployment::with_lock`
pub(crate) fn lock_deployment_session(
    conn: &PgConnection,
    site: &Site,
) -> Result<bool, StoreError> {
    WRITE.try_lock(conn, site.id)
}

/// Release the lock acquired with `lock_deployment_session`.
pub(crate) fn unlock_deployment_session(
    conn: &PgConnection,
    site: &Site,
) -> Result<(), StoreError> {
    WRITE.unlock(conn, site.id)
}

/// Try to take the lock used to prevent two prune operations from running at the
/// same time. Return `true` if we got the lock, and `false` otherwise.
pub(crate) fn try_lock_pruning(conn: &PgConnection, site: &Site) -> Result<bool, StoreError> {
    PRUNE.try_lock(conn, site.id)
}

pub(crate) fn unlock_pruning(conn: &PgConnection, site: &Site) -> Result<(), StoreError> {
    PRUNE.unlock(conn, site.id)
}
