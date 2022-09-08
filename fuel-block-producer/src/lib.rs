#![deny(unused_must_use)]

mod adapters;
pub mod block_producer;
pub mod config;
pub mod db;
pub mod ports;
pub mod service;

pub use config::Config;
pub use service::Service;

#[cfg(test)]
mod block_production_mock {
    use std::{
        borrow::Cow,
        net::{
            IpAddr,
            SocketAddr,
        },
        path::PathBuf,
        sync::Arc,
    };

    use anyhow::Result;
    use async_trait::async_trait;
    use rand::{
        prelude::StdRng,
        Rng,
        SeedableRng,
    };
    use tokio::sync::{
        broadcast,
        oneshot,
    };

    use super::{
        db::BlockProducerDatabase,
        ports::Relayer,
        Service,
    };
    use fuel_core::{
        chain_config::{
            ChainConfig,
            CoinConfig,
            ProductionStrategy,
            StateConfig,
        },
        database::Database,
        service::{
            Config as CoreServiceConfig,
            DbType,
            FuelService,
            VMConfig,
        },
    };
    use fuel_core_interfaces::{
        block_importer::ImportBlockBroadcast,
        block_producer::BlockProducerMpsc,
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
        },
        executor::{
            Error as ExecutorError,
            ExecutionMode,
            Executor,
        },
        model::{
            BlockHeight,
            DaBlockHeight,
            FuelBlock,
        },
    };
    use fuel_txpool::ServiceBuilder as TxPoolServiceBuilder;

    struct MockRelayer;

    #[async_trait]
    impl Relayer for MockRelayer {
        /// Get the block production key associated with a given
        /// validator id as of a specific block height
        async fn get_block_production_key(
            &self,
            _validator_id: Address,
            _da_height: DaBlockHeight,
        ) -> Result<Address> {
            Ok(Address::default())
        }

        /// Get the best finalized height from the DA layer
        async fn get_best_finalized_da_height(&self) -> Result<DaBlockHeight> {
            Ok(DaBlockHeight::default())
        }
    }

    struct MockExecutor;

    #[async_trait]
    impl Executor for MockExecutor {
        async fn execute(
            &self,
            _block: &mut FuelBlock,
            _mode: ExecutionMode,
        ) -> Result<(), ExecutorError> {
            Ok(())
        }
    }

    struct MockBlockProducerDatabase;

    impl BlockProducerDatabase for MockBlockProducerDatabase {
        /// fetch previously committed block at given height
        fn get_block(&self, _fuel_height: BlockHeight) -> Result<Cow<FuelBlock>> {
            todo!("Get block");
        }
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

    const COIN_AMOUNT: u64 = 1_000_000_000;

    fn make_tx(coin: &CoinInfo, gas_price: u64, gas_limit: u64) -> Transaction {
        TransactionBuilder::script(
            vec![Opcode::RET(REG_ZERO)].into_iter().collect(),
            vec![],
        )
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
        .finalize()
    }

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

        let service = Service::new(
            &crate::config::Config {
                max_gas_per_block,
                consensus_params,
                ..Default::default()
            },
            (),
        )
        .await?;

        let db = Database::in_memory();

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

        {
            let mut import_tx = db.transaction();
            let database = import_tx.as_mut();

            let config = CoreServiceConfig {
                addr: SocketAddr::from((IpAddr::from([0; 4]), 0)),
                database_path: PathBuf::new(),
                database_type: DbType::InMemory,
                chain_conf: ChainConfig {
                    block_production: ProductionStrategy::Manual,
                    chain_name: "test-chain".to_string(),
                    initial_state: Some(StateConfig {
                        coins: Some(
                            coins
                                .iter()
                                .map(|coin| CoinConfig {
                                    tx_id: Some(coin.id.into()),
                                    output_index: Some(coin.index.into()),
                                    block_created: Some(BlockHeight::from(0u32)),
                                    maturity: Some(BlockHeight::from(0u32)),
                                    owner: coin.address(),
                                    amount: COIN_AMOUNT,
                                    asset_id: AssetId::zeroed(),
                                })
                                .collect(),
                        ),
                        contracts: None,
                        height: None,
                        messages: None,
                    }),
                    transaction_parameters: consensus_params,
                },
                manual_blocks_enabled: false,
                predicates: false,
                utxo_validation: true,
                vm: VMConfig::default(),
                txpool: Default::default(),
                block_importer: Default::default(),
                block_producer: Default::default(),
                block_executor: Default::default(),
                bft: Default::default(),
                sync: Default::default(),
                #[cfg(feature = "relayer")]
                relayer: Default::default(),
                #[cfg(feature = "p2p")]
                p2p: Default::default(),
            };

            database.init(&config).unwrap();

            if let Some(initial_state) = &config.chain_conf.initial_state {
                FuelService::init_coin_state(database, initial_state)?;
            }

            import_tx.commit().unwrap();
        }

        let (import_block_events_tx, import_block_events_rx) = broadcast::channel(16);

        let mut txpool_builder = TxPoolServiceBuilder::new();
        txpool_builder.db(Box::new(db));
        txpool_builder.import_block_event(import_block_events_rx);
        let txpool = txpool_builder.build().unwrap();
        txpool.start().await?;

        service
            .start(
                Box::new(txpool.sender().clone()),
                Box::new(MockRelayer),
                Box::new(MockExecutor),
                Box::new(MockBlockProducerDatabase),
            )
            .await;

        // Add new transactions
        let txsize = make_tx(&coins[0], 1, 1).metered_bytes_size() as u64;

        // max_fee = gas_price * (txsize + limit)
        let gas_prices = [10, 20, 15];
        let min_fees = gas_prices.map(|g| g * txsize);
        let small_limit = gas_prices[0] * 2;
        assert!(
            min_fees[0] + min_fees[1] + small_limit * 2 < max_gas_per_block,
            "Incorrect test: no space in block"
        );
        let limit2_takes_whole_block = (max_gas_per_block / gas_prices[2])
            .checked_sub(txsize)
            .unwrap();

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
        let (resp_tx, resp_rx) = oneshot::channel();
        service
            .sender()
            .send(BlockProducerMpsc::Produce {
                height: BlockHeight::from(0u32),
                response: resp_tx,
            })
            .await
            .unwrap();

        let generated_block = resp_rx.await?.expect("Failed to generate block");

        // Check that the generated block looks right
        assert_eq!(generated_block.transactions.len(), 2);

        assert_eq!(generated_block.transactions[0].gas_price(), 20);
        assert_eq!(generated_block.transactions[1].gas_price(), 10);

        // Import the block to txpool
        import_block_events_tx
            .send(ImportBlockBroadcast::PendingFuelBlockImported {
                block: Arc::new((*Arc::try_unwrap(generated_block).unwrap()).clone()),
            })
            .expect("Failed to import the generated block");

        // Trigger block production again
        let (resp_tx, resp_rx) = oneshot::channel();
        service
            .sender()
            .send(BlockProducerMpsc::Produce {
                height: BlockHeight::from(1u32),
                response: resp_tx,
            })
            .await
            .unwrap();

        let generated_block = resp_rx.await?.expect("Failed to generate block");

        // Check that the generated block looks right
        assert_eq!(generated_block.transactions.len(), 1);
        assert_eq!(generated_block.transactions[0].gas_price(), 15);

        // Import the block to txpool
        import_block_events_tx
            .send(ImportBlockBroadcast::PendingFuelBlockImported {
                block: Arc::new((*Arc::try_unwrap(generated_block).unwrap()).clone()),
            })
            .expect("Failed to import the generated block");

        // Trigger block production once more, now the block should be empty
        let (resp_tx, resp_rx) = oneshot::channel();
        service
            .sender()
            .send(BlockProducerMpsc::Produce {
                height: BlockHeight::from(1u32),
                response: resp_tx,
            })
            .await
            .unwrap();

        let generated_block = resp_rx.await?.expect("Failed to generate block");

        // Check that the generated block looks right
        assert_eq!(generated_block.transactions.len(), 0);

        Ok(())
    }
}
