use fuel_core::service::FuelService;
use fuel_core_client::client::{
    types::CoinType,
    FuelClient,
};
use fuel_core_types::{
    fuel_asm::op,
    fuel_tx::{
        policies::Policies,
        Input,
        Transaction,
        TransactionBuilder,
        TxPointer,
    },
    services::executor::TransactionExecutionResult,
};
use test_helpers::{
    assemble_tx::AssembleAndRunTx,
    config_with_fee,
    default_signing_wallet,
};

#[tokio::test]
async fn assemble_transaction__witness_limit() {
    let config = config_with_fee();
    let service = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(service.bound_address);

    // Given
    let tx = TransactionBuilder::script(vec![op::ret(1)].into_iter().collect(), vec![])
        .witness_limit(10000)
        .finalize_as_transaction();

    // When
    let tx = client
        .assemble_transaction(&tx, default_signing_wallet(), vec![])
        .await
        .unwrap();
    let status = client.dry_run(&vec![tx]).await.unwrap();

    // Then
    let status = status.into_iter().next().unwrap();
    assert!(matches!(
        status.result,
        TransactionExecutionResult::Success { .. }
    ));
}

#[tokio::test]
async fn assemble_transaction__input_without_witness() {
    let config = config_with_fee();
    let base_asset_id = config.base_asset_id();
    let service = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(service.bound_address);
    let account = default_signing_wallet();
    let CoinType::Coin(coin) = client
        .coins_to_spend(&account.owner(), vec![(base_asset_id, 100, None)], None)
        .await
        .unwrap()[0][0]
    else {
        panic!("Expected a coin");
    };

    // Given
    let tx = Transaction::script(
        0,
        vec![],
        vec![],
        Policies::new(),
        vec![Input::coin_signed(
            coin.utxo_id,
            coin.owner,
            coin.amount,
            coin.asset_id,
            TxPointer::new(coin.block_created.into(), coin.tx_created_idx),
            0,
        )],
        vec![],
        vec![],
    );

    // When
    let tx = client
        .assemble_transaction(&tx.into(), account, vec![])
        .await
        .unwrap();
    let status = client.dry_run(&vec![tx]).await.unwrap();

    // Then
    let status = status.into_iter().next().unwrap();
    assert!(matches!(
        status.result,
        TransactionExecutionResult::Success { .. }
    ));
}
