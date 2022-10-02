use anyhow::Result;
use fuel_block_producer::{
    adapters::TxPoolAdapter,
    mocks::{
        MockDb,
        MockExecutor,
        MockRelayer,
    },
    Producer,
};
use fuel_core_interfaces::{
    block_importer::ImportBlockBroadcast,
    block_producer::BlockProducer as Trait,
    common::{
        fuel_asm::Opcode,
        fuel_crypto::{
            PublicKey,
            SecretKey,
        },
        fuel_merkle::common::Bytes32,
        fuel_tx::{
            ConsensusParameters,
            Output,
            Transaction,
            TransactionBuilder,
            UtxoId,
        },
        fuel_types::{
            Address,
            AssetId,
        },
        fuel_vm::consts::REG_ZERO,
        prelude::StorageAsMut,
    },
    db::Coins,
    model::{
        Coin,
        CoinStatus,
    },
    txpool::Sender as TxPoolSender,
};
use fuel_txpool::{
    Config as TxPoolConfig,
    MockDb as TxPoolDb,
    ServiceBuilder as TxPoolServiceBuilder,
};
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};
use std::sync::Arc;
use tokio::sync::{
    broadcast,
    mpsc,
};

const COIN_AMOUNT: u64 = 1_000_000_000;

#[tokio::test]
async fn block_producer() -> Result<()> {
    let mut rng = StdRng::seed_from_u64(1234u64);

    let consensus_params = ConsensusParameters {
        contract_max_size: 10000,
        gas_per_byte: 1,
        gas_price_factor: 1,
        max_gas_per_tx: 1_000_000,
        max_inputs: 16,
        max_message_data_length: 16,
        max_outputs: 16,
        max_predicate_data_length: 10000,
        max_predicate_length: 10000,
        max_script_data_length: 10000,
        max_script_length: 10000,
        max_storage_slots: 10000,
        max_witnesses: 16,
    };

    let max_gas_per_block = 1_000_000;

    let mut txpool_db = TxPoolDb::default();

    let coins: Vec<_> = (0..3)
        .map(|index| {
            let id = rng.gen();
            let secret_key = SecretKey::random(&mut rng);
            CoinInfo {
                index: index + 1,
                id,
                secret_key,
            }
        })
        .collect();

    for coin in &coins {
        txpool_db
            .storage::<Coins>()
            .insert(
                &UtxoId::new(coin.id.into(), coin.index),
                &Coin {
                    owner: coin.address(),
                    amount: COIN_AMOUNT,
                    asset_id: AssetId::zeroed(),
                    maturity: 0u32.into(),
                    status: CoinStatus::Unspent,
                    block_created: 0u32.into(),
                },
            )
            .expect("unable to insert seed coin data");
    }

    let (import_block_events_tx, import_block_events_rx) = broadcast::channel(16);

    let mut txpool_builder = TxPoolServiceBuilder::new();

    let (tx_status_sender, mut tx_status_receiver) = broadcast::channel(100);

    // Remove once tx_status events are used
    tokio::spawn(async move { while (tx_status_receiver.recv().await).is_ok() {} });

    let (txpool_sender, txpool_receiver) = mpsc::channel(100);
    let (incoming_tx_sender, incoming_tx_receiver) = broadcast::channel(100);

    let keep_alive = Box::new(incoming_tx_sender);
    Box::leak(keep_alive);

    txpool_builder
        .config(TxPoolConfig::default())
        .db(Box::new(txpool_db))
        .incoming_tx_receiver(incoming_tx_receiver)
        .import_block_event(import_block_events_rx)
        .tx_status_sender(tx_status_sender)
        .txpool_sender(TxPoolSender::new(txpool_sender))
        .txpool_receiver(txpool_receiver);

    let (p2p_request_event_sender, _p2p_request_event_receiver) = mpsc::channel(100);
    txpool_builder.network_sender(p2p_request_event_sender);

    let txpool = txpool_builder.build().unwrap();
    txpool.start().await?;

    let mock_db = MockDb::default();

    let block_producer = Producer {
        config: fuel_block_producer::config::Config {
            max_gas_per_block,
            consensus_params,
        },
        db: &mock_db,
        txpool: &TxPoolAdapter {
            sender: txpool.sender().clone(),
            consensus_params,
        },
        executor: &MockExecutor(mock_db.clone()),
        relayer: &MockRelayer::default(),
        lock: Default::default(),
    };

    // Add new transactions
    let txsize = make_tx(&coins[0], 1, 1).metered_bytes_size() as u64
        * consensus_params.gas_per_byte;

    let small_limit = 100;
    assert!(
        (txsize + small_limit) * 2 < max_gas_per_block,
        "Incorrect test: no space in block"
    );
    let limit2_takes_whole_block = max_gas_per_block.checked_sub(txsize).unwrap();
    let gas_prices = [10, 20, 15];
    let results: Vec<_> = txpool
        .sender()
        .insert(
            coins
                .iter()
                .zip([
                    (gas_prices[0], small_limit),
                    (gas_prices[1], small_limit),
                    (gas_prices[2], limit2_takes_whole_block),
                ]) // Produces blocks [1, 0] and [2]
                .map(|(coin, (gas_price, gas_limit))| {
                    Arc::new(make_tx(coin, gas_price, gas_limit))
                })
                .collect(),
        )
        .await
        .expect("Couldn't insert transaction")
        .into_iter()
        .map(|r| r.expect("Invalid tx"))
        .collect();

    assert_eq!(results, vec![vec![], vec![], vec![]]);

    // Trigger block production
    let generated_block = block_producer
        .produce_block(1u32.into())
        .await
        .expect("Failed to generate block");

    // Check that the generated block looks right
    assert_eq!(generated_block.transactions.len(), 2);

    assert_eq!(generated_block.transactions[0].gas_price(), 20);
    assert_eq!(generated_block.transactions[1].gas_price(), 10);

    // Import the block to txpool
    import_block_events_tx
        .send(ImportBlockBroadcast::PendingFuelBlockImported {
            block: Arc::new(generated_block.clone()),
        })
        .expect("Failed to import the generated block");

    // Trigger block production again
    let generated_block = block_producer
        .produce_block(2u32.into())
        .await
        .expect("Failed to generate block");

    // Check that the generated block looks right
    assert_eq!(generated_block.transactions.len(), 1);
    assert_eq!(generated_block.transactions[0].gas_price(), 15);

    // Import the block to txpool
    import_block_events_tx
        .send(ImportBlockBroadcast::PendingFuelBlockImported {
            block: Arc::new(generated_block.clone()),
        })
        .expect("Failed to import the generated block");

    // Trigger block production once more, now the block should be empty
    let generated_block = block_producer
        .produce_block(3u32.into())
        .await
        .expect("Failed to generate block");

    // Check that the generated block looks right
    assert_eq!(generated_block.transactions.len(), 0);

    Ok(())
}

struct CoinInfo {
    index: u8,
    id: Bytes32,
    secret_key: SecretKey,
}

impl CoinInfo {
    pub fn public_key(&self) -> PublicKey {
        self.secret_key.public_key()
    }

    pub fn address(&self) -> Address {
        Address::new(self.public_key().hash().into())
    }

    pub fn utxo_id(&self) -> UtxoId {
        UtxoId::new(self.id.into(), self.index)
    }
}

fn make_tx(coin: &CoinInfo, gas_price: u64, gas_limit: u64) -> Transaction {
    TransactionBuilder::script(vec![Opcode::RET(REG_ZERO)].into_iter().collect(), vec![])
        .gas_price(gas_price)
        .gas_limit(gas_limit)
        .add_unsigned_coin_input(
            coin.secret_key,
            coin.utxo_id(),
            COIN_AMOUNT,
            AssetId::zeroed(),
            Default::default(),
            0,
        )
        .add_output(Output::Change {
            to: Default::default(),
            amount: 0,
            asset_id: AssetId::zeroed(),
        })
        .finalize_without_signature()
}
