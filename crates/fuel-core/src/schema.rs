use crate::fuel_core_graphql_api::{
    api_service::ReadDatabase,
    database::ReadView,
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        query,
        Connection,
        CursorType,
        Edge,
        EmptyFields,
    },
    parser::types::OperationType,
    Context,
    MergedObject,
    MergedSubscription,
    OutputType,
    Schema,
    SchemaBuilder,
};
use fuel_core_storage::{
    iter::IterDirection,
    Result as StorageResult,
};
use itertools::Itertools;
use std::borrow::Cow;

pub mod balance;
pub mod blob;
pub mod block;
pub mod chain;
pub mod coins;
pub mod contract;
pub mod dap;
pub mod health;
pub mod message;
pub mod node_info;
pub mod upgrades;

pub mod gas_price;
pub mod scalars;
pub mod tx;

pub mod relayed_tx;

#[derive(MergedObject, Default)]
pub struct Query(
    dap::DapQuery,
    balance::BalanceQuery,
    blob::BlobQuery,
    block::BlockQuery,
    chain::ChainQuery,
    tx::TxQuery,
    health::HealthQuery,
    coins::CoinQuery,
    contract::ContractQuery,
    contract::ContractBalanceQuery,
    node_info::NodeQuery,
    gas_price::LatestGasPriceQuery,
    gas_price::EstimateGasPriceQuery,
    message::MessageQuery,
    relayed_tx::RelayedTransactionQuery,
    upgrades::UpgradeQuery,
);

#[derive(MergedObject, Default)]
pub struct Mutation(dap::DapMutation, tx::TxMutation, block::BlockMutation);

#[derive(MergedSubscription, Default)]
pub struct Subscription(tx::TxStatusSubscription);

pub type CoreSchema = Schema<Query, Mutation, Subscription>;
pub type CoreSchemaBuilder = SchemaBuilder<Query, Mutation, Subscription>;

pub fn build_schema() -> CoreSchemaBuilder {
    Schema::build_with_ignore_name_conflicts(
        Query::default(),
        Mutation::default(),
        Subscription::default(),
        ["TransactionConnection", "MessageConnection"],
    )
}

async fn query_pagination<F, Entries, SchemaKey, SchemaValue>(
    after: Option<String>,
    before: Option<String>,
    first: Option<i32>,
    last: Option<i32>,
    entries: F,
) -> async_graphql::Result<Connection<SchemaKey, SchemaValue, EmptyFields, EmptyFields>>
where
    SchemaKey: CursorType + Send + Sync,
    <SchemaKey as CursorType>::Error: core::fmt::Display + Send + Sync + 'static,
    SchemaValue: OutputType,
    // TODO: Optimization: Support `count` here including skipping of entities.
    //  It means also returning `has_previous_page` and `has_next_page` values.
    // entries(start_key: Option<DBKey>)
    F: FnOnce(&Option<SchemaKey>, IterDirection) -> StorageResult<Entries>,
    Entries: Iterator<Item = StorageResult<(SchemaKey, SchemaValue)>>,
    SchemaKey: Eq,
{
    match (after.as_ref(), before.as_ref(), first, last) {
        (_, _, Some(first), Some(last)) => {
            return Err(anyhow!(
                "Either first `{first}` or latest `{last}` elements, not both"
            )
            .into())
        }
        (Some(after), _, _, Some(last)) => {
            return Err(anyhow!(
                "After `{after:?}` with last `{last}` elements is not supported"
            )
            .into())
        }
        (_, Some(before), Some(first), _) => {
            return Err(anyhow!(
                "Before `{before:?}` with first `{first}` elements is not supported"
            )
            .into())
        }
        (_, _, None, None) => {
            return Err(anyhow!("The queries for the whole range is not supported").into())
        }
        (_, _, _, _) => { /* Other combinations are allowed */ }
    };

    query(
        after,
        before,
        first,
        last,
        |after: Option<SchemaKey>, before: Option<SchemaKey>, first, last| async move {
            let (count, direction) = if let Some(first) = first {
                (first, IterDirection::Forward)
            } else if let Some(last) = last {
                (last, IterDirection::Reverse)
            } else {
                return Err(anyhow!("Either `first` or `last` should be provided"))
            };

            let start;
            let end;

            if direction == IterDirection::Forward {
                start = after;
                end = before;
            } else {
                start = before;
                end = after;
            }

            let entries = entries(&start, direction)?;
            let mut has_previous_page = false;
            let mut has_next_page = false;

            // TODO: Add support of `skip` field for pages with huge list of entities with
            //  the same `SchemaKey`.
            let entries = entries.skip_while(|result| {
                if let Ok((key, _)) = result {
                    // TODO: `entries` should return information about `has_previous_page` for wild
                    //  queries
                    if let Some(start) = start.as_ref() {
                        // Skip until start + 1
                        if key == start {
                            has_previous_page = true;
                            return true
                        }
                    }
                }
                false
            });

            let mut count = count.saturating_add(1) /* for `has_next_page` */;
            let entries = entries.take(count).take_while(|result| {
                if let Ok((key, _)) = result {
                    if let Some(end) = end.as_ref() {
                        // take until we've reached the end
                        if key == end {
                            has_next_page = true;
                            return false
                        }
                    }
                    count = count.saturating_sub(1);
                    has_next_page |= count == 0;
                    count != 0
                } else {
                    // We want to stop immediately in the case of error
                    false
                }
            });

            let entries: Vec<_> = entries.try_collect()?;
            let entries = entries.into_iter();

            let mut connection = Connection::new(has_previous_page, has_next_page);

            connection.edges.extend(
                entries
                    .into_iter()
                    .map(|(key, value)| Edge::new(key, value)),
            );

            Ok::<Connection<SchemaKey, SchemaValue>, anyhow::Error>(connection)
        },
    )
    .await
}

pub trait ReadViewProvider {
    /// Returns the read view for the current operation.
    fn read_view(&self) -> StorageResult<Cow<ReadView>>;
}

impl<'a> ReadViewProvider for Context<'a> {
    fn read_view(&self) -> StorageResult<Cow<'a, ReadView>> {
        let operation_type = self.query_env.operation.node.ty;

        // Sometimes, during mutable queries or subscription the resolvers
        // need access to an updated view of the database.
        if operation_type != OperationType::Query {
            let database: &ReadDatabase = self.data_unchecked();
            database.view().map(Cow::Owned)
        } else {
            let read_view: &ReadView = self.data_unchecked();
            Ok(Cow::Borrowed(read_view))
        }
    }
}
