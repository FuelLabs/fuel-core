use crate::service::adapters::consensus_module::poa::pre_confirmation_signature::Preconfirmations;
use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoaError,
        Result as PoAResult,
    },
    tx_receiver::{
        TxReceiver,
        TxSender,
    },
};
use fuel_core_types::{
    fuel_tx::TxId,
    services::p2p::PreconfirmationStatus,
};

pub struct MPSCTxReceiver<T> {
    sender: tokio::sync::mpsc::Sender<T>,
    receiver: tokio::sync::mpsc::Receiver<T>,
}

impl<T> MPSCTxReceiver<T> {
    pub fn new() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        MPSCTxReceiver { sender, receiver }
    }
}

#[derive(Clone)]
pub struct MPSCTxSender<T> {
    sender: tokio::sync::mpsc::Sender<T>,
}

impl TxReceiver for MPSCTxReceiver<Vec<(TxId, PreconfirmationStatus)>> {
    type Txs = Preconfirmations;
    type Sender = MPSCTxSender<Vec<(TxId, PreconfirmationStatus)>>;

    async fn receive(&mut self) -> PoAResult<Self::Txs> {
        self.receiver.recv().await.ok_or(PoaError::TxReceiver(
            "Failed to receive transaction, channel closed".to_string(),
        ))
    }

    fn get_sender(&self) -> Self::Sender {
        MPSCTxSender {
            sender: self.sender.clone(),
        }
    }
}

impl TxSender for MPSCTxSender<Vec<(TxId, PreconfirmationStatus)>> {
    type Txs = Preconfirmations;

    async fn send(&mut self, txs: Self::Txs) -> PoAResult<()> {
        self.sender
            .send(txs)
            .await
            .map_err(|e| PoaError::TxReceiver(format!("{}", e)))
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

        let mut receiver = MPSCTxReceiver::new();
        let mut sender = receiver.get_sender().unwrap();

        // when
        sender.send(txs.clone()).await.unwrap();

        // then
        let received_txs = receiver.receive().await.unwrap();
        assert_eq!(txs, received_txs);
    }
}
