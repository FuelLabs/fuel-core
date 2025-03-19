use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoaError,
        Result as PoAResult,
    },
    tx_receiver::TxReceiver,
};
use fuel_core_types::services::preconfirmation::Preconfirmation;
use tokio::sync::mpsc;

pub struct PreconfirmationsReceiver {
    capacity: usize,
    receiver: mpsc::Receiver<Vec<Preconfirmation>>,
}

impl Default for PreconfirmationsReceiver {
    fn default() -> Self {
        let (_, receiver) = mpsc::channel(1);
        Self::new(receiver)
    }
}

// TODO(#2739): Remove when integrated
// link: https://github.com/FuelLabs/fuel-core/issues/2739
#[allow(dead_code)]
impl PreconfirmationsReceiver {
    pub fn new(receiver: mpsc::Receiver<Vec<Preconfirmation>>) -> Self {
        let capacity = receiver.capacity();
        PreconfirmationsReceiver { capacity, receiver }
    }
}

impl TxReceiver for PreconfirmationsReceiver {
    type Txs = Vec<Preconfirmation>;

    async fn receive(&mut self) -> PoAResult<Self::Txs> {
        let mut buffer = Vec::new();
        let received = self.receiver.recv_many(&mut buffer, self.capacity).await;

        if received == 0 {
            return Err(PoaError::TxReceiver(
                "Failed to receive transaction, channel closed".to_string(),
            ));
        }

        Ok(buffer.into_iter().flatten().collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use std::sync::Arc;

    use super::*;
    use fuel_core_types::{
        fuel_tx::TxId,
        services::{
            preconfirmation::PreconfirmationStatus,
            transaction_status::statuses::{
                PreConfirmationSqueezedOut,
                PreConfirmationSuccess,
            },
        },
    };

    #[tokio::test]
    async fn receive__gets_what_is_sent_through_channel() {
        // given
        let txs = vec![
            Preconfirmation {
                tx_id: TxId::default(),
                status: PreconfirmationStatus::SqueezedOut(Arc::new(
                    PreConfirmationSqueezedOut {
                        reason: "Dummy reason".to_string(),
                    },
                )),
            },
            Preconfirmation {
                tx_id: TxId::default(),
                status: PreconfirmationStatus::Success(Arc::new(
                    PreConfirmationSuccess {
                        tx_pointer: Default::default(),
                        total_gas: 0,
                        total_fee: 0,
                        receipts: Some(vec![]),
                        outputs: Some(vec![]),
                    },
                )),
            },
        ];

        let (sender, receiver) = mpsc::channel(1);

        let mut receiver = PreconfirmationsReceiver::new(receiver);

        // when
        sender.send(txs.clone()).await.unwrap();

        // then
        let received_txs = receiver.receive().await.unwrap();
        assert_eq!(txs, received_txs);
    }
}
