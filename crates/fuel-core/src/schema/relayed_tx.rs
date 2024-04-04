// TODO: Remove all these before merging
#![allow(unused_imports)]
#![allow(unreachable_code)]
#![allow(unused_variables)]

use crate::{
    fuel_core_graphql_api::{
        database::ReadView,
        IntoApiResult,
    },
    schema::{
        scalars::{
            Bytes32,
            Nonce,
            Tai64Timestamp,
            U32,
        },
        tx::types::{
            FailureStatus,
            SqueezedOutStatus,
            SubmittedStatus,
            SuccessStatus,
        },
    },
};
use async_graphql::{
    Context,
    Enum,
    Object,
    Union,
};
use fuel_core_types::{
    entities,
    entities::relayer::transaction::RelayedTransactionStatus as FuelRelayedTransactionStatus,
    fuel_types::BlockHeight,
    tai64::Tai64,
};

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
        let status =
            crate::query::relayed_tx_status(query, id.0)?.map(|status| status.into());
        Ok(status)
    }
}

#[derive(Union, Debug)]
pub enum RelayedTransactionStatus {
    Failed(RelayedTransactionFailed),
}

#[derive(Debug)]
pub struct RelayedTransactionFailed {
    pub block_height: BlockHeight,
    pub block_time: Tai64,
    pub failure: String,
}

#[Object]
impl RelayedTransactionFailed {
    async fn block_height(&self) -> U32 {
        let as_u32: u32 = self.block_height.into();
        as_u32.into()
    }

    async fn block_time(&self) -> Tai64Timestamp {
        self.block_time.into()
    }

    async fn failure(&self) -> String {
        self.failure.clone()
    }
}

impl From<FuelRelayedTransactionStatus> for RelayedTransactionStatus {
    fn from(status: FuelRelayedTransactionStatus) -> Self {
        match status {
            FuelRelayedTransactionStatus::Failed {
                block_height,
                block_time,
                failure,
            } => RelayedTransactionStatus::Failed(RelayedTransactionFailed {
                block_height,
                block_time,
                failure,
            }),
        }
    }
}
