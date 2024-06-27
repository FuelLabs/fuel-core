use crate::{
    fuel_core_graphql_api::{
        ports::DatabaseRelayedTransactions,
        QUERY_COSTS,
    },
    schema::{
        scalars::{
            RelayedTransactionId,
            U32,
        },
        ReadViewProvider,
    },
};
use async_graphql::{
    Context,
    Object,
    Union,
};
use fuel_core_types::{
    entities::relayer::transaction::RelayedTransactionStatus as FuelRelayedTransactionStatus,
    fuel_types::BlockHeight,
};

#[derive(Default)]
pub struct RelayedTransactionQuery {}

#[Object]
impl RelayedTransactionQuery {
    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn relayed_transaction_status(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The id of the relayed tx")] id: RelayedTransactionId,
    ) -> async_graphql::Result<Option<RelayedTransactionStatus>> {
        let query = ctx.read_view()?;
        let status = query.transaction_status(id.0)?.map(|status| status.into());
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
    pub failure: String,
}

#[Object]
impl RelayedTransactionFailed {
    async fn block_height(&self) -> U32 {
        let as_u32: u32 = self.block_height.into();
        as_u32.into()
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
                failure,
            } => RelayedTransactionStatus::Failed(RelayedTransactionFailed {
                block_height,
                failure,
            }),
        }
    }
}
