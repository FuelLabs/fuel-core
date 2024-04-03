use async_graphql::{
    Enum,
    Object,
};
use fuel_core_types::entities;

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
