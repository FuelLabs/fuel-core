//! This crate implements the store for all chain and subgraph data. See the
//! [Store] for the details of how the store is organized across
//! different databases/shards.

#[macro_use]
extern crate derive_more;
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;
#[macro_use]
extern crate diesel_derive_enum;

mod advisory_lock;
mod block_range;
mod block_store;
mod catalog;
mod chain_head_listener;
mod chain_store;
pub mod connection_pool;
mod copy;
mod deployment;
mod deployment_store;
mod detail;
mod dynds;
mod fork;
mod functions;
mod jobs;
mod jsonb;
mod notification_listener;
mod primary;
pub mod query_store;
mod relational;
mod relational_queries;
mod retry;
mod store;
mod store_events;
mod subgraph_store;
pub mod transaction_receipt;
mod writable;

#[cfg(debug_assertions)]
pub mod layout_for_tests {
    pub use crate::block_range::*;
    pub use crate::block_store::FAKE_NETWORK_SHARED;
    pub use crate::catalog::set_account_like;
    pub use crate::primary::{
        make_dummy_site, Connection, Mirror, Namespace, EVENT_TAP, EVENT_TAP_ENABLED,
    };
    pub use crate::relational::*;
    pub mod writable {
        pub use crate::writable::test_support::allow_steps;
    }
}

pub use self::block_store::BlockStore;
pub use self::chain_head_listener::ChainHeadUpdateListener;
pub use self::chain_store::{ChainStore, ChainStoreMetrics};
pub use self::detail::DeploymentDetail;
pub use self::jobs::register as register_jobs;
pub use self::notification_listener::NotificationSender;
pub use self::primary::{db_version, UnusedDeployment};
pub use self::store::Store;
pub use self::store_events::SubscriptionManager;
pub use self::subgraph_store::{unused, DeploymentPlacer, Shard, SubgraphStore, PRIMARY_SHARD};

/// This module is only meant to support command line tooling. It must not
/// be used in 'normal' graph-node code
pub mod command_support {
    pub mod catalog {
        pub use crate::block_store::primary as block_store;
        pub use crate::catalog::{account_like, stats};
        pub use crate::copy::{copy_state, copy_table_state};
        pub use crate::primary::{
            active_copies, deployment_schemas, ens_names, subgraph, subgraph_deployment_assignment,
            subgraph_version, Site,
        };
        pub use crate::primary::{Connection, Mirror};
    }
    pub mod index {
        pub use crate::relational::index::{CreateIndex, Method};
    }
    pub use crate::deployment::{on_sync, OnSync};
    pub use crate::primary::Namespace;
    pub use crate::relational::{Catalog, Column, ColumnType, Layout, SqlName};
}
