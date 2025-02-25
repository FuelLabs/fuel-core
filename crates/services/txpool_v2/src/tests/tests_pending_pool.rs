use fuel_core_services::Service;
use fuel_core_types::fuel_tx::{
    UniqueIdentifier,
    UtxoId,
};

use crate::tests::universe::TestPoolUniverse;

#[tokio::test]
async fn test_tx_keep_missing_input() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;

    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 10);
    let tx_id1 = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx_id1, 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    // When
    tokio::time::timeout(
        tokio::time::Duration::from_secs(1),
        universe.waiting_txs_insertion(
            service.shared.new_tx_notification_subscribe(),
            vec![tx2.id(&Default::default())],
        ),
    )
    .await
    .unwrap_err();
    service.shared.try_insert(vec![tx1.clone()]).unwrap();

    universe
        .waiting_txs_insertion(
            service.shared.new_tx_notification_subscribe(),
            vec![tx1.id(&Default::default()), tx2.id(&Default::default())],
        )
        .await;

    // Then
    service.stop_and_await().await.unwrap();
}
