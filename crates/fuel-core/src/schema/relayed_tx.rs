// TODO: Remove all these before merging
#![allow(unused_imports)]
#![allow(unreachable_code)]
#![allow(unused_variables)]

use crate::{
    fuel_core_graphql_api::database::ReadView,
    schema::scalars::{
        Bytes32,
        Nonce,
    },
};
use async_graphql::{
    Context,
    Enum,
    Object,
};
use fuel_core_types::entities;

#[derive(Default)]
pub struct RelayedTransactionQuery {}

#[Object]
impl RelayedTransactionQuery {
    async fn status(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The id of the relayed tx")] id: Bytes32,
    ) -> async_graphql::Result<Option<RelayedTransactionStatus>> {
        let query: &ReadView = ctx.data_unchecked();
        let status = crate::query::relayed_tx_status(query, id.0)?;
        Ok(status.into())
    }
}

pub struct RelayedTransactionStatus(
    pub(crate) entities::relayer::transaction::RelayedTransactionStatus,
);

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum RelayedTransactionState {
    Failed,
}

#[Object]
impl RelayedTransactionStatus {
    async fn state(&self) -> RelayedTransactionState {
        match &self.0 {
            entities::relayer::transaction::RelayedTransactionStatus::Failed {
                block_height: _,
                block_time: _,
                failure: _,
            } => RelayedTransactionState::Failed,
        }
    }
}
