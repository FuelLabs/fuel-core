//! Transfer tests for the e2e test client.

async fn can_send_funds_from_a_to_b() {
    let config = config::SuiteConfig::default();
    let client_a = Client::new(config.wallet_a.clone());
    let client_b = Client::new(config.wallet_b.clone());

    let amount = 100;
    let tx = client_a
        .transfer(
            config.wallet_b.secret.to_public_key(),
            amount,
            config.wallet_a.secret,
        )
        .await
        .unwrap();

    let receipt = client_a
        .wait_for_receipt(tx, config.wallet_a.secret)
        .await
        .unwrap();

    assert_eq!(receipt.status, ReceiptStatus::Success);

    let balance_a = client_a.get_balance(config.wallet_a.secret).await.unwrap();
    let balance_b = client_b.get_balance(config.wallet_b.secret).await.unwrap();

    assert_eq!(balance_a, 0);
    assert_eq!(balance_b, amount);
}
