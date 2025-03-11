use fuel_core_services::Service as ServiceTrait;

use crate::tests::universe::TestTxStatusManagerUniverse;

#[tokio::test]
async fn test_start_stop() {
    let service = TestTxStatusManagerUniverse::default().build_service(None);
    service.start_and_await().await.unwrap();

    // Double start will return false.
    assert!(service.start().is_err(), "double start should fail");

    let state = service.stop_and_await().await.unwrap();
    assert!(state.stopped());
}

// Tests to implement: https://github.com/FuelLabs/fuel-core/issues/2838
// 1. `status_update()` sends correct notifications for `tx_status_change` and
//    `new_tx_notification_sender` depending on the `TransactionStatus` variant.
// 2. `status_update()` updates the `statuses` map with the correct `TransactionStatus`.
//     for example `status_update()` does not update the `statuses` map
//     with the `TransactionStatus::SqueezedOut` (if we decide so)
// 3. Pruning of old statuses
//    - upon storage in DB
//    - other pruning scenarios if we decide to implement them
// 4. `status_update()` is called from all components: TxPool, Consensus, WorkerService and P2P
// 5. Signature is verified before the preconfirmations are accepted
