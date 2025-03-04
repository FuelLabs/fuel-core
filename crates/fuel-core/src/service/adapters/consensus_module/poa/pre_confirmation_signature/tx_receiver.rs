use crate::service::adapters::consensus_module::poa::pre_confirmation_signature::Preconfirmations;
use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoaError,
        Result as PoAResult,
    },
    tx_receiver::TxReceiver,
};
use fuel_core_types::{
    fuel_tx::TxId,
    services::p2p::PreconfirmationStatus,
};

// TODO(#2739): Remove when integrated
// link: https://github.com/FuelLabs/fuel-core/issues/2739
#[allow(dead_code)]
pub struct MPSCTxReceiver<T> {
    receiver: tokio::sync::mpsc::Receiver<T>,
}

impl TxReceiver for MPSCTxReceiver<Vec<(TxId, PreconfirmationStatus)>> {
    type Txs = Preconfirmations;

    async fn receive(&mut self) -> PoAResult<Self::Txs> {
        self.receiver.recv().await.ok_or(PoaError::TxReceiver(
            "Failed to receive transaction, channel closed".to_string(),
        ))
    }
}
#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use fuel_core_types::fuel_types::BlockHeight;

    #[tokio::test]
    async fn receive__gets_what_is_sent_through_channel() {
        // given
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let txs = vec![
            (
                TxId::default(),
                PreconfirmationStatus::SuccessByBlockProducer {
                    block_height: BlockHeight::from(123),
                },
            ),
            (
                TxId::default(),
                PreconfirmationStatus::SqueezedOutByBlockProducer {
                    reason: "test".to_string(),
                },
            ),
        ];

        let mut receiver = MPSCTxReceiver { receiver };

        // when
        sender.send(txs.clone()).await.unwrap();

        // then
        let received_txs = receiver.receive().await.unwrap();
        assert_eq!(txs, received_txs);
    }
}
