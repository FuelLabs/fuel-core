use std::{
    hash::{
        DefaultHasher,
        Hash,
        Hasher,
    },
    time::Duration,
};

use fuel_core::{
    chain_config::TESTNET_WALLET_SECRETS,
    p2p_test_helpers::{
        make_nodes,
        BootstrapSetup,
        Nodes,
        ProducerSetup,
        ValidatorSetup,
    },
    service::Config,
};
use fuel_core_client::client::{
    types::{
        assemble_tx::{
            ChangePolicy,
            RequiredBalance,
        },
        TransactionStatus,
    },
    FuelClient,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        RegId,
    },
    fuel_tx::{
        Address,
        AssetId,
        Input,
        Output,
        Receipt,
        TransactionBuilder,
        TxPointer,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::BlockHeight,
    fuel_vm::SecretKey,
};
use futures::StreamExt;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use test_helpers::assemble_tx::{
    AssembleAndRunTx,
    SigningAccount,
};

fn config_with_preconfirmations(block_production_period: Duration) -> Config {
    let mut config = Config::local_node();

    config.p2p.as_mut().unwrap().subscribe_to_pre_confirmations = true;
    config.txpool.pending_pool_tx_ttl = Duration::from_secs(1);
    config
        .pre_confirmation_signature_service
        .echo_delegation_interval = Duration::from_millis(100);
    config.block_production = Trigger::Open {
        period: block_production_period,
    };

    config
}

#[tokio::test(flavor = "multi_thread")]
async fn preconfirmation__propagate_p2p_after_successful_execution() {
    let mut rng = rand::thread_rng();
    let address = Address::new([0; 32]);
    let block_production_period = Duration::from_secs(8);
    let gas_limit = 1_000_000;
    let amount = 10;

    // Given
    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    // Given
    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            amount,
            AssetId::default(),
            Default::default(),
        )
        .add_output(Output::change(address, 0, AssetId::default()))
        .finalize_as_transaction();
    let tx_id = tx.id(&Default::default());
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    let num_partitions = 1;
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create a producer for each key pair and a set of validators that share
    // the same key pair.
    let Nodes {
        producers: _producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_name(format!("{}:producer", pub_keys[i]))
                    .utxo_validation(false),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        .utxo_validation(false),
                )
            })
        }),
        Some(config_with_preconfirmations(block_production_period)),
    )
    .await;

    let sentry = &validators[0];

    // Sleep to let time for exchange preconfirmations delegate public keys
    tokio::time::sleep(Duration::from_secs(1)).await;

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    let mut tx_statuses_subscriber = client_sentry
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .expect("Should be able to subscribe for events");

    // Then
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));

    let status =
        tokio::time::timeout(Duration::from_secs(1), tx_statuses_subscriber.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    if let TransactionStatus::PreconfirmationSuccess {
        tx_pointer,
        total_fee,
        total_gas: _,
        transaction_id,
        receipts,
        resolved_outputs,
    } = status.clone()
    {
        // Then
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 1));
        assert_eq!(total_fee, 0);
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 3);
        assert!(matches!(receipts[0],
            Receipt::Log {
                ra, rb, ..
            } if ra == 0xca && rb == 0xba));

        assert!(matches!(receipts[1],
            Receipt::Return {
                val, ..
            } if val == 1));
        let outputs = resolved_outputs.unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0].output,
            Output::Change {
                to: address,
                amount,
                asset_id: AssetId::default()
            }
        );
    } else {
        panic!("Expected preconfirmation status, got {status:?}");
    }
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Success { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn preconfirmation__propagate_p2p_after_failed_execution() {
    let mut rng = rand::thread_rng();
    let address = Address::new([0; 32]);
    let block_production_period = Duration::from_secs(8);
    let gas_limit = 1_000_000;
    let amount = 10;

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::rvrt(RegId::ONE),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    // Given
    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            amount,
            AssetId::default(),
            Default::default(),
        )
        .add_output(Output::change(address, 0, AssetId::default()))
        .finalize_as_transaction();
    let tx_id = tx.id(&Default::default());
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    let num_partitions = 1;
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create a producer for each key pair and a set of validators that share
    // the same key pair.
    let Nodes {
        producers: _producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_name(format!("{}:producer", pub_keys[i]))
                    .utxo_validation(false),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        .utxo_validation(false),
                )
            })
        }),
        Some(config_with_preconfirmations(block_production_period)),
    )
    .await;

    let sentry = &validators[0];

    // Sleep to let time for exchange preconfirmations delegate public keys
    tokio::time::sleep(Duration::from_secs(1)).await;

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    let mut tx_statuses_subscriber = client_sentry
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .expect("Should be able to subscribe for events");

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));

    let status =
        tokio::time::timeout(Duration::from_secs(1), tx_statuses_subscriber.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();

    if let TransactionStatus::PreconfirmationFailure {
        tx_pointer,
        total_fee,
        total_gas: _,
        transaction_id,
        receipts,
        resolved_outputs,
        reason: _,
    } = status
    {
        // Then
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 1));
        assert_eq!(total_fee, 0);
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 3);
        assert!(matches!(receipts[0],
            Receipt::Log {
                ra, rb, ..
            } if ra == 0xca && rb == 0xba));

        assert!(matches!(receipts[1],
            Receipt::Revert {
                ra, ..
            } if ra == 1));
        let outputs = resolved_outputs.unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0].output,
            Output::Change {
                to: address,
                amount,
                asset_id: AssetId::default()
            }
        );
    } else {
        panic!("Expected preconfirmation status");
    }

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Failure { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn preconfirmation__propagate_p2p_after_squeezed_out_on_producer() {
    let mut rng = rand::thread_rng();

    let block_production_period = Duration::from_secs(8);
    let gas_limit = 1_000_000;
    let tx = TransactionBuilder::script(
        vec![op::ret(RegId::ONE)].into_iter().collect(),
        vec![],
    )
    .script_gas_limit(gas_limit)
    .add_unsigned_coin_input(
        SecretKey::random(&mut rng),
        rng.gen(),
        10,
        Default::default(),
        Default::default(),
    )
    .add_output(Output::Change {
        to: Default::default(),
        amount: 0,
        asset_id: Default::default(),
    })
    .finalize_as_transaction();

    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    let num_partitions = 1;
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create a producer for each key pair and a set of validators that share
    // the same key pair.

    // Given
    // Disable UTXO validation in TxPool so that the transaction is squeezed out by
    // block production
    let Nodes {
        producers: _producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_name(format!("{}:producer", pub_keys[i]))
                    .utxo_validation(true),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        .utxo_validation(false),
                )
            })
        }),
        Some(config_with_preconfirmations(block_production_period)),
    )
    .await;

    let sentry = &validators[0];

    // Sleep to let time for exchange preconfirmations delegate public keys
    tokio::time::sleep(Duration::from_secs(1)).await;

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    let mut tx_statuses_subscriber = client_sentry
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .expect("Should be able to subscribe for events");

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    tracing::info!("Submitted");

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::SqueezedOut { .. }
    ));
    tracing::info!("SqueezedOut");
    let tx = client_sentry
        .transaction(&tx.id(&Default::default()))
        .await
        .unwrap();
    assert!(tx.is_none())
}

#[tokio::test(flavor = "multi_thread")]
async fn preconfirmation__sentry_allows_usage_of_dynamic_outputs() {
    // Set production to be long to be sure that we only work with pre confirmations without
    // final confirmation.
    let block_production_period = Duration::from_secs(30);
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    let num_partitions = 1;
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Enabled UTXO validation because this test rely on real coins.
    let mut config = config_with_preconfirmations(block_production_period);
    config.gas_price_config.min_exec_gas_price = 1000;

    let base_asset_id = config.base_asset_id();
    let chain_id = config.chain_id();

    let Nodes {
        producers: _producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_name(format!("{}:producer", pub_keys[i]))
                    .utxo_validation(true),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        .utxo_validation(true),
                )
            })
        }),
        Some(config),
    )
    .await;

    let sentry = &validators[0];
    let client_sentry = FuelClient::from(sentry.node.bound_address);

    // Sleep to let time for exchange preconfirmations delegate public keys
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Below we create a transfer with 3 outputs where:
    // 1. The first output is `Coin` to the recipient address with known amount.
    // 2. The second output is `Change` to the funding address where all remaining funds go.
    // 3. The third output is `Variable` to the recipient address with variable amount created
    //  via script.
    //
    // We want to test that the next transaction can use all 3 outputs as inputs
    // and also be pre-confirmed.
    let receiver_secret = SecretKey::random(&mut rng);
    let receiver = Input::owner(&receiver_secret.public_key());
    let variable_amount = 10000;
    let known_amount = 123;

    let asset_id_register = 0x10;
    let balance_register = 0x11;
    let output_index_register = 0x12;
    let recipient_id_register = 0x13;

    // Script transfer `variable_amount` of base asset to the receiver.
    let script = vec![
        op::gtf_args(asset_id_register, 0x00, GTFArgs::ScriptData),
        // Load recipient's address
        op::addi(
            recipient_id_register,
            asset_id_register,
            u16::try_from(AssetId::LEN).expect("The size is 32"),
        ),
        op::movi(balance_register, variable_amount),
        // The first(0) output is `Coin` to the recipient address with known amount.
        // The second(1) output is `Change` to the funding address.
        // The third(2) output is `Variable` to the recipient address with variable amount.
        op::movi(output_index_register, 2),
        op::tro(
            recipient_id_register,
            output_index_register,
            balance_register,
            asset_id_register,
        ),
        op::ret(RegId::ONE),
    ];

    let script_data = base_asset_id
        .iter()
        .copied()
        .chain(receiver.iter().cloned())
        .collect();

    let funds_secret_key: SecretKey = TESTNET_WALLET_SECRETS[1].parse().unwrap();
    let funded_account = SigningAccount::Wallet(funds_secret_key);

    let script_with_transfers = TransactionBuilder::script(script.into_iter().collect(), script_data)
            // We want to shift all dynamic outputs like `Change` and `Variable`
            // to the right for 1 output, so we add known output as a first one to achieve it.
            .add_output(Output::coin(receiver, known_amount, base_asset_id))
            .finalize_as_transaction();
    let required_balances = vec![RequiredBalance {
        asset_id: base_asset_id,
        amount: variable_amount.into(),
        account: funded_account.clone().into_account(),
        change_policy: ChangePolicy::Change(funded_account.owner()),
    }];

    // Assembling of the transactions adds `Change` and `Variable` outputs.
    let final_script_with_transfers = client_sentry
        .assemble_transaction(&script_with_transfers, funded_account, required_balances)
        .await
        .unwrap();

    let tx_statuses_subscriber = client_sentry
        .submit_and_await_status_opt(&final_script_with_transfers, None, Some(true))
        .await
        .expect("Should be able to subscribe for events");

    let status = tx_statuses_subscriber
        .skip(1)
        .next()
        .await
        .unwrap()
        .unwrap();
    let TransactionStatus::PreconfirmationSuccess {
        tx_pointer,
        resolved_outputs,
        ..
    } = status
    else {
        panic!("Expected preconfirmation status, but got {status:?}");
    };
    let resolved_outputs = resolved_outputs.unwrap();

    // Given
    // Crafting a dependent transaction that relies on 3 outputs from the previous transaction.
    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder.max_fee_limit(10_000);

    // Add `Change` output of the funding account as an input
    let Output::Change {
        amount, asset_id, ..
    } = resolved_outputs[0].output
    else {
        panic!("Expected change output")
    };
    builder.add_unsigned_coin_input(
        funds_secret_key,
        resolved_outputs[0].utxo_id,
        amount,
        asset_id,
        tx_pointer,
    );

    // Add `Variable` output of the receiver account as an input
    let Output::Variable {
        amount, asset_id, ..
    } = resolved_outputs[1].output
    else {
        panic!("Expected change output")
    };
    builder.add_unsigned_coin_input(
        receiver_secret,
        resolved_outputs[1].utxo_id,
        amount,
        asset_id,
        tx_pointer,
    );

    // Add known `Coin` output of the receiver account as an input
    let transfers_tx_id = final_script_with_transfers.id(&chain_id);
    let known_utxo_id = UtxoId::new(transfers_tx_id, 0);
    builder.add_unsigned_coin_input(
        receiver_secret,
        known_utxo_id,
        known_amount,
        base_asset_id,
        tx_pointer,
    );

    let transaction_that_rely_on_variable_inputs = builder.finalize_as_transaction();

    // When
    let mut tx_statuses_subscriber = client_sentry
        .submit_and_await_status_opt(
            &transaction_that_rely_on_variable_inputs,
            None,
            Some(true),
        )
        .await
        .expect("Should be able to subscribe for events");

    // Then
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    let status = tx_statuses_subscriber.next().await.unwrap().unwrap();
    assert!(
        matches!(status, TransactionStatus::PreconfirmationSuccess { .. }),
        "Expected preconfirmation status, but got {status:?}"
    );
}
