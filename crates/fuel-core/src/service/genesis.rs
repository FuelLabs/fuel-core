use self::workers::GenesisWorkers;
use crate::{
    database::{
        genesis_progress::GenesisMetadata,
        Database,
    },
    service::config::Config,
};
use anyhow::anyhow;
use fuel_core_chain_config::{
    CoinConfig,
    ContractConfig,
    GenesisCommitment,
    MessageConfig,
};
use fuel_core_storage::{
    structured_storage::TableWithBlueprint,
    tables::{
        Coins,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        Messages,
    },
    transactional::{
        StorageTransaction,
        Transactional,
    },
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::{
            Consensus,
            Genesis,
        },
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::{
            DaBlockHeight,
            Empty,
        },
        SealedBlock,
    },
    entities::{
        coins::coin::Coin,
        contract::{
            ContractUtxoInfo,
            ContractsInfoType,
        },
        message::Message,
    },
    fuel_tx::Contract,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::block_importer::{
        ImportResult,
        UncommittedResult as UncommittedImportResult,
    },
};

pub mod off_chain;
mod runner;
mod workers;

pub use runner::{
    GenesisRunner,
    TransactionOpener,
};

/// Performs the importing of the genesis block from the snapshot.
pub async fn execute_genesis_block(
    config: &Config,
    original_database: &Database,
) -> anyhow::Result<UncommittedImportResult<StorageTransaction<Database>>> {
    let workers =
        GenesisWorkers::new(original_database.clone(), config.state_reader.clone());

    import_chain_state(workers).await?;

    let genesis = Genesis {
        chain_config_hash: config.chain_config.root()?.into(),
        coins_root: original_database.genesis_coins_root()?.into(),
        messages_root: original_database.genesis_messages_root()?.into(),
        contracts_root: original_database.genesis_contracts_root()?.into(),
    };

    let block = create_genesis_block(config);
    let consensus = Consensus::Genesis(genesis);
    let block = SealedBlock {
        entity: block,
        consensus,
    };

    let mut database_transaction = Transactional::transaction(original_database);
    cleanup_genesis_progress(database_transaction.as_mut())?;

    let result = UncommittedImportResult::new(
        ImportResult::new_from_local(block, vec![], vec![]),
        database_transaction,
    );

    Ok(result)
}

async fn import_chain_state(workers: GenesisWorkers) -> anyhow::Result<()> {
    if let Err(e) = workers.run_imports().await {
        workers.shutdown();
        tokio::select! {
            _ = workers.finished() => {}
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                return Err(anyhow!("Timeout while importing genesis state"));
            }
        };

        return Err(e);
    }

    Ok(())
}

fn cleanup_genesis_progress(database: &mut Database) -> anyhow::Result<()> {
    database
        .delete_all(GenesisMetadata::column())
        .map_err(|e| e.into())
}

pub fn create_genesis_block(config: &Config) -> Block {
    let block_height = config.state_reader.block_height();
    let da_block_height = config.state_reader.da_block_height();
    Block::new(
        PartialBlockHeader {
            application: ApplicationHeader::<Empty> {
                da_height: da_block_height,
                generated: Empty,
            },
            consensus: ConsensusHeader::<Empty> {
                // The genesis is a first block, so previous root is zero.
                prev_root: Bytes32::zeroed(),
                // The block height at genesis.
                height: block_height,
                time: fuel_core_types::tai64::Tai64::UNIX_EPOCH,
                generated: Empty,
            },
        },
        // Genesis block doesn't have any transaction.
        vec![],
        &[],
    )
}

#[cfg(feature = "test-helpers")]
pub async fn execute_and_commit_genesis_block(
    config: &Config,
    original_database: &Database,
) -> anyhow::Result<()> {
    let result = execute_genesis_block(config, original_database).await?;
    let importer = fuel_core_importer::Importer::new(
        config.block_importer.clone(),
        original_database.clone(),
        (),
        (),
    );
    importer.commit_result(result).await?;
    Ok(())
}

fn init_coin(
    db: &mut Database,
    coin: &CoinConfig,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let utxo_id = coin.utxo_id();

    let compressed_coin = Coin {
        utxo_id,
        owner: coin.owner,
        amount: coin.amount,
        asset_id: coin.asset_id,
        tx_pointer: coin.tx_pointer(),
    }
    .compress();

    // ensure coin can't point to blocks in the future
    let coin_height = coin.tx_pointer().block_height();
    if coin_height > height {
        return Err(anyhow!(
            "coin tx_pointer height ({coin_height}) cannot be greater than genesis block ({height})"
        ));
    }

    if db
        .storage::<Coins>()
        .insert(&utxo_id, &compressed_coin)?
        .is_some()
    {
        return Err(anyhow!("Coin should not exist"));
    }

    Ok(())
}

fn init_contract(
    db: &mut Database,
    contract_config: &ContractConfig,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let contract = Contract::from(contract_config.code.as_slice());
    let salt = contract_config.salt;
    let contract_id = contract_config.contract_id;
    #[allow(clippy::cast_possible_truncation)]
    let utxo_id = contract_config.utxo_id();

    let tx_pointer = contract_config.tx_pointer();
    if tx_pointer.block_height() > height {
        return Err(anyhow!(
            "contract tx_pointer cannot be greater than genesis block"
        ));
    }

    // insert contract code
    if db
        .storage::<ContractsRawCode>()
        .insert(&contract_id, contract.as_ref())?
        .is_some()
    {
        return Err(anyhow!("Contract code should not exist"));
    }

    // insert contract salt
    if db
        .storage::<ContractsInfo>()
        .insert(&contract_id, &ContractsInfoType::V1(salt.into()))?
        .is_some()
    {
        return Err(anyhow!("Contract info should not exist"));
    }
    if db
        .storage::<ContractsLatestUtxo>()
        .insert(
            &contract_id,
            &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
        )?
        .is_some()
    {
        return Err(anyhow!("Contract utxo should not exist"));
    }

    Ok(())
}

fn init_da_message(
    db: &mut Database,
    msg: MessageConfig,
    da_height: DaBlockHeight,
) -> anyhow::Result<()> {
    let message: Message = msg.into();

    if message.da_height() > da_height {
        return Err(anyhow!(
            "message da_height cannot be greater than genesis da block height"
        ));
    }

    if db
        .storage::<Messages>()
        .insert(message.id(), &message)?
        .is_some()
    {
        return Err(anyhow!("Message should not exist"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        combined_database::CombinedDatabase,
        database::genesis_progress::GenesisResource,
        service::{
            config::Config,
            FuelService,
            Task,
        },
    };
    use fuel_core_chain_config::{
        ChainConfig,
        CoinConfig,
        ContractBalanceConfig,
        ContractConfig,
        ContractStateConfig,
        MessageConfig,
        StateConfig,
        StateReader,
    };
    use fuel_core_services::RunnableService;
    use fuel_core_storage::{
        tables::{
            Coins,
            ContractsAssets,
            ContractsState,
        },
        StorageAsRef,
    };
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        entities::coins::coin::Coin,
        fuel_asm::op,
        fuel_tx::UtxoId,
        fuel_types::{
            Address,
            AssetId,
            BlockHeight,
            ContractId,
            Salt,
        },
        fuel_vm::Contract,
    };
    use itertools::Itertools;
    use rand::{
        rngs::StdRng,
        Rng,
        RngCore,
        SeedableRng,
    };
    use std::vec;

    #[tokio::test]
    async fn config_initializes_block_height() {
        let block_height = BlockHeight::from(99u32);
        let state_reader = StateReader::in_memory(StateConfig {
            block_height,
            ..Default::default()
        });
        let service_config = Config {
            state_reader,
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        assert_eq!(
            block_height,
            db.latest_height()
                .expect("Expected a block height to be set")
        )
    }

    #[tokio::test]
    async fn genesis_columns_are_cleared_after_import() {
        let mut rng = StdRng::seed_from_u64(10);

        let coins = (0..1000)
            .map(|_| CoinConfig {
                amount: 10,
                tx_id: rng.gen(),
                ..Default::default()
            })
            .collect_vec();

        let messages = (0..1000)
            .map(|_| MessageConfig {
                sender: rng.gen(),
                recipient: rng.gen(),
                nonce: rng.gen(),
                amount: rng.gen(),
                data: vec![rng.gen()],
                da_height: DaBlockHeight(0),
            })
            .collect_vec();

        let contracts = (0..1000)
            .map(|_| given_contract_config(&mut rng))
            .collect_vec();

        let contract_state = (0..1000)
            .map(|_| ContractStateConfig {
                contract_id: rng.gen(),
                key: rng.gen(),
                value: [1, 2, 3].to_vec(),
            })
            .collect_vec();

        let contract_balance = (0..1000)
            .map(|_| ContractBalanceConfig {
                contract_id: rng.gen(),
                asset_id: rng.gen(),
                amount: rng.next_u64(),
            })
            .collect_vec();

        let state = StateConfig {
            coins,
            messages,
            contracts,
            contract_state,
            contract_balance,
            block_height: BlockHeight::from(0u32),
            da_block_height: Default::default(),
        };
        let state_reader = StateReader::in_memory(state);

        let service_config = Config {
            state_reader,
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        use strum::IntoEnumIterator;
        for key in GenesisResource::iter() {
            assert!(db.genesis_progress(&key).is_none());
        }
    }

    #[tokio::test]
    async fn config_state_initializes_multiple_coins_with_different_owners_and_asset_ids()
    {
        let mut rng = StdRng::seed_from_u64(10);

        // a coin with all options set
        let alice: Address = rng.gen();
        let asset_id_alice: AssetId = rng.gen();
        let alice_value = rng.gen();
        let alice_block_created: BlockHeight = rng.next_u32().into();
        let alice_block_created_tx_idx = rng.gen();
        let alice_tx_id = rng.gen();
        let alice_output_index = rng.gen();
        let alice_utxo_id = UtxoId::new(alice_tx_id, alice_output_index);

        // a coin with minimal options set
        let bob: Address = rng.gen();
        let asset_id_bob: AssetId = rng.gen();
        let bob_value = rng.gen();

        let starting_height = {
            let mut h: u32 = alice_block_created.into();
            h = h.saturating_add(rng.next_u32());
            h.into()
        };
        let state = StateConfig {
            coins: vec![
                CoinConfig {
                    tx_id: alice_tx_id,
                    output_index: alice_output_index,
                    tx_pointer_block_height: alice_block_created,
                    tx_pointer_tx_idx: alice_block_created_tx_idx,
                    owner: alice,
                    amount: alice_value,
                    asset_id: asset_id_alice,
                },
                CoinConfig {
                    owner: bob,
                    amount: bob_value,
                    asset_id: asset_id_bob,
                    ..Default::default()
                },
            ],
            block_height: starting_height,
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state);

        let service_config = Config {
            state_reader,
            ..Config::local_node()
        };

        let db = CombinedDatabase::default();
        FuelService::from_combined_database(db.clone(), service_config)
            .await
            .unwrap();

        let alice_coins = get_coins(&db, &alice);
        let bob_coins = get_coins(&db, &bob);

        assert!(matches!(
            alice_coins.as_slice(),
            &[Coin {
                utxo_id,
                owner,
                amount,
                asset_id,
                tx_pointer,
                ..
            }] if utxo_id == alice_utxo_id
            && owner == alice
            && amount == alice_value
            && asset_id == asset_id_alice
            && tx_pointer.block_height() == alice_block_created
        ));
        assert!(matches!(
            bob_coins.as_slice(),
            &[Coin {
                owner,
                amount,
                asset_id,
                ..
            }] if owner == bob
            && amount == bob_value
            && asset_id == asset_id_bob
        ));
    }

    #[tokio::test]
    async fn config_state_initializes_contract_state() {
        let mut rng = StdRng::seed_from_u64(10);

        let salt: Salt = rng.gen();
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &Contract::default_state_root());

        let test_key = rng.gen();
        let test_value = [1, 2, 3].to_vec();
        let contract_state = ContractStateConfig {
            contract_id,
            key: test_key,
            value: test_value.clone(),
        };

        let state = StateConfig {
            contracts: vec![ContractConfig {
                contract_id,
                code: contract.into(),
                salt,
                tx_id: rng.gen(),
                output_index: rng.gen(),
                tx_pointer_block_height: 0.into(),
                tx_pointer_tx_idx: rng.gen(),
            }],
            contract_state: vec![contract_state],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state);

        let service_config = Config {
            chain_config: ChainConfig::local_testnet(),
            state_reader,
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        let ret = db
            .storage::<ContractsState>()
            .get(&(&contract_id, &test_key).into())
            .unwrap()
            .expect("Expect a state entry to exist with test_key")
            .into_owned();

        assert_eq!(test_value.to_vec(), ret.0)
    }

    #[cfg(feature = "test-helpers")]
    #[tokio::test]
    async fn tests_init_da_msgs() {
        let mut rng = StdRng::seed_from_u64(32492);

        let msg = MessageConfig {
            sender: rng.gen(),
            recipient: rng.gen(),
            nonce: rng.gen(),
            amount: rng.gen(),
            data: vec![rng.gen()],
            da_height: DaBlockHeight(0),
        };

        let state = StateConfig {
            messages: vec![msg.clone()],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state);

        let config = Config {
            state_reader,
            ..Config::local_node()
        };

        let db = &Database::default();

        execute_and_commit_genesis_block(&config, db).await.unwrap();

        let expected_msg: Message = msg.into();

        let ret_msg = db
            .as_ref()
            .storage::<fuel_core_storage::tables::Messages>()
            .get(expected_msg.id())
            .unwrap()
            .unwrap()
            .into_owned();

        assert_eq!(expected_msg, ret_msg);
    }

    #[tokio::test]
    async fn config_state_initializes_contract_balance() {
        let mut rng = StdRng::seed_from_u64(10);

        let salt: Salt = rng.gen();
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &Contract::default_state_root());

        let test_asset_id = rng.gen();
        let test_balance = rng.next_u64();
        let contract_balance = ContractBalanceConfig {
            contract_id,
            asset_id: test_asset_id,
            amount: test_balance,
        };

        let state = StateConfig {
            contracts: vec![ContractConfig {
                contract_id,
                code: contract.into(),
                salt,
                ..Default::default()
            }],
            contract_balance: vec![contract_balance],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state);

        let service_config = Config {
            chain_config: ChainConfig::local_testnet(),
            state_reader,
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        let ret = db
            .storage::<ContractsAssets>()
            .get(&(&contract_id, &test_asset_id).into())
            .unwrap()
            .expect("Expected a balance to be present")
            .into_owned();

        assert_eq!(test_balance, ret)
    }

    #[tokio::test]
    async fn coin_tx_pointer_cant_exceed_genesis_height() {
        let state = StateConfig {
            coins: vec![CoinConfig {
                // set txpointer height > genesis height
                tx_pointer_block_height: BlockHeight::from(11u32),
                amount: 10,
                ..Default::default()
            }],
            block_height: BlockHeight::from(10u32),
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state);

        let service_config = Config {
            state_reader,
            ..Config::local_node()
        };

        let db = CombinedDatabase::default();
        let task = Task::new(db, service_config).unwrap();
        let init_result = task.into_task(&Default::default(), ()).await;

        assert!(init_result.is_err())
    }

    #[tokio::test]
    async fn contract_tx_pointer_cant_exceed_genesis_height() {
        let mut rng = StdRng::seed_from_u64(10);

        let test_asset_id: AssetId = rng.gen();
        let test_balance: u64 = rng.next_u64();
        let contract_id = Default::default();
        let balances = vec![ContractBalanceConfig {
            contract_id,
            asset_id: test_asset_id,
            amount: test_balance,
        }];
        let salt: Salt = rng.gen();
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());

        let state = StateConfig {
            contracts: vec![ContractConfig {
                contract_id: ContractId::from(*contract_id),
                code: contract.into(),
                salt,
                // set txpointer height > genesis height
                tx_pointer_block_height: BlockHeight::from(11u32),
                tx_pointer_tx_idx: 0,
                ..Default::default()
            }],
            contract_balance: balances,
            block_height: BlockHeight::from(10u32),
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state);

        let service_config = Config {
            state_reader,
            ..Config::local_node()
        };

        let db = CombinedDatabase::default();
        let task = Task::new(db, service_config).unwrap();
        let init_result = task.into_task(&Default::default(), ()).await;

        assert!(init_result.is_err())
    }

    fn get_coins(db: &CombinedDatabase, owner: &Address) -> Vec<Coin> {
        db.off_chain()
            .owned_coins_ids(owner, None, None)
            .map(|r| {
                let coin_id = r.unwrap();
                db.on_chain()
                    .storage::<Coins>()
                    .get(&coin_id)
                    .map(|v| v.unwrap().into_owned().uncompress(coin_id))
                    .unwrap()
            })
            .collect()
    }

    fn given_contract_config(rng: &mut StdRng) -> ContractConfig {
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());
        let salt = rng.gen();
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &Contract::default_state_root());

        ContractConfig {
            contract_id,
            code: contract.into(),
            salt,
            ..Default::default()
        }
    }

    fn given_contract_config_with_tx(rng: &mut StdRng) -> ContractConfig {
        let mut config = given_contract_config(rng);
        config.tx_id = rng.gen();
        config.output_index = rng.gen();
        config.tx_pointer_block_height = 0.into();
        config.tx_pointer_tx_idx = rng.gen();

        config
    }

    #[tokio::test]
    async fn config_state_initializes_contract() {
        let mut rng = StdRng::seed_from_u64(10);
        let config = given_contract_config_with_tx(&mut rng);
        let contract_id = config.contract_id;

        let state = StateConfig {
            contracts: vec![config.clone()],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state);

        let service_config = Config {
            state_reader,
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        let initialized_contract = db.get_contract_config(contract_id).unwrap();

        assert_eq!(initialized_contract, config);
    }
}
