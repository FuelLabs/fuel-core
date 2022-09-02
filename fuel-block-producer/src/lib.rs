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
        collections::{BTreeMap, HashMap},
        sync::Arc,
    };

    use anyhow::Result;
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use rand::prelude::StdRng;
    use rand::{Rng, SeedableRng};
    use tokio::sync::{broadcast, mpsc, oneshot};

    use super::{db::BlockProducerDatabase, ports::Relayer, Config, Service};
    use fuel_core_interfaces::{
        block_producer::BlockProducerMpsc,
        common::{
            fuel_asm::Opcode,
            fuel_crypto::SecretKey,
            fuel_storage::Storage,
            fuel_tx::{Output, Transaction, TransactionBuilder, UtxoId},
            fuel_types::Address,
            fuel_vm::consts::{REG_ONE, REG_ZERO},
        },
        db::helpers::{Data, DummyDb, TX_ID1},
        executor::{Error as ExecutorError, ExecutionMode, Executor},
        model::{BlockHeight, Coin, CoinStatus, DaBlockHeight, FuelBlock},
        txpool::{Sender as TxPoolSender, TxPoolMpsc},
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
            println!("GET-BP-KEY");
            Ok(Address::default())
        }

        /// Get the best finalized height from the DA layer
        async fn get_best_finalized_da_height(&self) -> Result<DaBlockHeight> {
            println!("GET-BEST-HEIGHT");
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
            println!("MOCK-EXECUTE");
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

    #[tokio::test]
    async fn block_producer() -> Result<()> {
        let mut rng = StdRng::seed_from_u64(1234u64);

        let service = Service::new(&Config::default(), ()).await?;

        let mut db = DummyDb {
            data: Arc::new(Mutex::new(Data {
                validators: HashMap::new(),
                validators_height: DaBlockHeight::from(0u32),
                staking_diffs: BTreeMap::new(),
                delegator_index: BTreeMap::new(),
                sealed_blocks: HashMap::new(),
                chain_height: BlockHeight::from(0u32),
                finalized_da_height: DaBlockHeight::from(0u32),
                tx_hashes: Vec::new(),
                tx: HashMap::new(),
                coins: HashMap::new(),
                contract: HashMap::new(),
                messages: HashMap::new(),
                last_committed_finalized_fuel_height: BlockHeight::from(0u32),
            })),
        };

        let tx = Transaction::default();
        let value = Coin {
            owner: Address::default(),
            amount: 400,
            asset_id: Default::default(),
            maturity: BlockHeight::from(0u64),
            status: CoinStatus::Unspent,
            block_created: BlockHeight::from(0u64),
        };
        let coin_key = UtxoId::new(tx.id(), 0);
        let old_coin = db.insert(&coin_key, &value)?;
        assert!(old_coin.is_none());

        let (_import_block_events_tx, import_block_events_rx) = broadcast::channel(16);

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

        // Add a new transaction
        let results: Vec<_> = txpool
            .sender()
            .insert(vec![Arc::new(
                // DummyDb::dummy_tx(*TX_ID1),
                TransactionBuilder::script(
                    vec![Opcode::RET(REG_ZERO)].into_iter().collect(),
                    vec![],
                )
                .gas_price(100)
                .gas_limit(1000)
                .add_unsigned_coin_input(
                    SecretKey::random(&mut rng),
                    coin_key,
                    10_000,
                    Default::default(),
                    Default::default(),
                    0,
                )
                .add_output(Output::Change {
                    to: Default::default(),
                    amount: 0,
                    asset_id: Default::default(),
                })
                .finalize(),
            )])
            .await
            .expect("Couldn't insert transaction")
            .into_iter()
            .map(|r| r.expect("Invalid tx"))
            .collect();
        assert_eq!(results, vec![vec![], vec![]]);

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

        let generated_block = resp_rx.await??;
        // TODO: check that the block is right

        assert_eq!(generated_block.transactions.len(), 2);
        dbg!(generated_block);
        panic!();

        Ok(())
    }
}
