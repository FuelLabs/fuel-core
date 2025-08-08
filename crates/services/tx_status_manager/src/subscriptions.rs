use async_trait::async_trait;
use tokio_stream::StreamExt;
use fuel_core_services::stream::BoxStream;

use crate::ports::P2PPreConfirmationGossipData;

#[async_trait]
pub trait TxStatusSubscription: Send {
    async fn next_tx_status(&mut self) -> Option<P2PPreConfirmationGossipData>;
}
pub struct Subscriptions {
    pub new_tx_status: BoxStream<P2PPreConfirmationGossipData>,
}

#[async_trait]
impl TxStatusSubscription for Subscriptions {
    async fn next_tx_status(&mut self) -> Option<P2PPreConfirmationGossipData> {
        self.new_tx_status.next().await
    }
}