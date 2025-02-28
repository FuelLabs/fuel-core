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
