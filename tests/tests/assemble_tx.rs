use fuel_core::{
    chain_config::{
        ChainConfig,
        StateConfig,
        TESTNET_WALLET_SECRETS,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::{
        assemble_tx::{
            ChangePolicy,
            RequiredBalance,
        },
        CoinType,
    },
    FuelClient,
};
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_asm::op,
    fuel_crypto::SecretKey,
    fuel_tx::{
        policies::Policies,
        Address,
        AssetId,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
        TxPointer,
    },
    services::executor::TransactionExecutionResult,
};
use test_helpers::{
    assemble_tx::{
        AssembleAndRunTx,
        SigningAccount,
    },
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
async fn assemble_transaction__preserves_users_variable_output_even_if_it_is_empty() {
    let config = config_with_fee();
    let base_asset_id = config.base_asset_id();
    let service = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(service.bound_address);
    let secret: SecretKey = TESTNET_WALLET_SECRETS[1].parse().unwrap();
    let account = SigningAccount::Wallet(secret);
    let CoinType::Coin(coin) = client
        .coins_to_spend(&account.owner(), vec![(base_asset_id, 100, None)], None)
        .await
        .unwrap()[0][0]
    else {
        panic!("Expected a coin");
    };

    // Given
    let tx: Transaction =
        TransactionBuilder::script(vec![op::ret(1)].into_iter().collect(), vec![])
            .add_unsigned_coin_input(
                secret,
                coin.utxo_id,
                coin.amount,
                coin.asset_id,
                TxPointer::new(coin.block_created.into(), coin.tx_created_idx),
            )
            .add_output(Output::change(account.owner(), 0, base_asset_id))
            .add_output(Output::variable(Default::default(), 0, Default::default()))
            .finalize_as_transaction();

    // When
    let tx = client
        .assemble_transaction(&tx, default_signing_wallet(), vec![])
        .await
        .unwrap();
    let status = client.dry_run(&vec![tx.clone()]).await.unwrap();
    let status = status.into_iter().next().unwrap();
    assert!(matches!(
        status.result,
        TransactionExecutionResult::Success { .. }
    ));

    // Then
    let outputs = tx.outputs();
    assert_eq!(outputs.len(), 2);
    assert!(outputs[0].is_change());
    assert!(outputs[1].is_variable());
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

#[tokio::test]
async fn assemble_transaction__user_provided_change_output() {
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
        vec![Output::Change {
            asset_id: base_asset_id,
            to: coin.owner,
            amount: 0,
        }],
        vec![],
    );

    // When
    let tx = client
        .assemble_transaction(
            &tx.into(),
            account.clone(),
            vec![RequiredBalance {
                asset_id: base_asset_id,
                amount: 0,
                account: account.clone().into_account(),
                change_policy: ChangePolicy::Change(account.owner()),
            }],
        )
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
async fn assemble_transaction__transfer_non_based_asset() {
    let mut state_config = StateConfig::local_testnet();
    let chain_config = ChainConfig::local_testnet();

    let secret: SecretKey = TESTNET_WALLET_SECRETS[1].parse().unwrap();
    let account = SigningAccount::Wallet(secret);
    let owner = account.owner();
    let base_asset_id = *chain_config.consensus_parameters.base_asset_id();
    let non_base_asset_id = AssetId::from([1; 32]);
    assert_ne!(base_asset_id, non_base_asset_id);

    // Given
    state_config.coins[0].owner = owner;
    state_config.coins[0].asset_id = base_asset_id;
    state_config.coins[1].owner = owner;
    state_config.coins[1].asset_id = non_base_asset_id;

    let mut config = Config::local_node_with_configs(chain_config, state_config);
    config.utxo_validation = true;
    config.gas_price_config.min_exec_gas_price = 1000;

    let service = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(service.bound_address);

    // Given
    let recipient = Address::new([123; 32]);
    let amount = 5_000;
    let tx = TransactionBuilder::script(vec![op::ret(1)].into_iter().collect(), vec![])
        .add_output(Output::Coin {
            to: recipient,
            asset_id: non_base_asset_id,
            amount,
        })
        .finalize_as_transaction();

    // When
    let tx = client
        .assemble_transaction(
            &tx,
            default_signing_wallet(),
            vec![RequiredBalance {
                asset_id: non_base_asset_id,
                amount,
                account: account.clone().into_account(),
                change_policy: ChangePolicy::Change(owner),
            }],
        )
        .await
        .unwrap();
    let status = client.dry_run(&vec![tx.clone()]).await.unwrap();

    // Then
    let status = status.into_iter().next().unwrap();
    assert!(matches!(
        status.result,
        TransactionExecutionResult::Success { .. }
    ));
}

#[tokio::test]
async fn assemble_transaction__adds_change_output_for_non_required_non_base_balance() {
    let mut state_config = StateConfig::local_testnet();
    let chain_config = ChainConfig::local_testnet();

    let secret: SecretKey = TESTNET_WALLET_SECRETS[1].parse().unwrap();
    let account = SigningAccount::Wallet(secret);
    let owner = account.owner();
    let base_asset_id = *chain_config.consensus_parameters.base_asset_id();
    let non_base_asset_id = AssetId::from([1; 32]);
    assert_ne!(base_asset_id, non_base_asset_id);

    // Given
    state_config.coins[0].owner = owner;
    state_config.coins[0].asset_id = base_asset_id;
    state_config.coins[1].owner = owner;
    state_config.coins[1].asset_id = non_base_asset_id;

    let mut config = Config::local_node_with_configs(chain_config, state_config);
    config.utxo_validation = true;
    config.gas_price_config.min_exec_gas_price = 1000;

    let service = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(service.bound_address);
    let CoinType::Coin(coin) = client
        .coins_to_spend(&owner, vec![(non_base_asset_id, 100, None)], None)
        .await
        .unwrap()[0][0]
    else {
        panic!("Expected a coin");
    };

    // Given
    let tx = TransactionBuilder::script(vec![op::ret(1)].into_iter().collect(), vec![])
        .add_unsigned_coin_input(
            secret,
            coin.utxo_id,
            coin.amount,
            coin.asset_id,
            TxPointer::new(coin.block_created.into(), coin.tx_created_idx),
        )
        .finalize_as_transaction();

    // When
    let tx = client
        .assemble_transaction(&tx, default_signing_wallet(), vec![])
        .await
        .unwrap();
    let status = client.dry_run(&vec![tx.clone()]).await.unwrap();
    let status = status.into_iter().next().unwrap();
    assert!(matches!(
        status.result,
        TransactionExecutionResult::Success { .. }
    ));

    // Then
    let outputs = tx.outputs();
    assert_eq!(outputs.len(), 2);
    assert!(outputs[0].is_change());
    assert_eq!(outputs[0].asset_id(), Some(&non_base_asset_id));
    assert!(outputs[1].is_change());
    assert_eq!(outputs[1].asset_id(), Some(&base_asset_id));
}
