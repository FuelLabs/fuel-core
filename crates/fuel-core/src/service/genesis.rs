use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
        },
        genesis_progress::GenesisMetadata,
    },
    service::config::Config,
};
use fuel_core_chain_config::GenesisCommitment;
use fuel_core_services::StateWatcher;
use fuel_core_storage::{
    iter::IteratorOverTable,
    tables::{
        ConsensusParametersVersions,
        StateTransitionBytecodeVersions,
        UploadedBytecodes,
    },
    transactional::{
        Changes,
        IntoTransaction,
        ReadTransaction,
    },
    StorageAsMut,
};
use fuel_core_types::{
    self,
    blockchain::{
        block::Block,
        consensus::{
            Consensus,
            Genesis,
        },
        header::{
            ApplicationHeader,
            ConsensusHeader,
            ConsensusParametersVersion,
            PartialBlockHeader,
            StateTransitionBytecodeVersion,
        },
        primitives::Empty,
        SealedBlock,
    },
    fuel_crypto::Hasher,
    fuel_types::Bytes32,
    fuel_vm::UploadedBytecode,
    services::block_importer::{
        ImportResult,
        UncommittedResult as UncommittedImportResult,
    },
};
use itertools::Itertools;

mod exporter;
mod importer;
mod progress;
mod task_manager;

pub use exporter::Exporter;
pub use task_manager::NotifyCancel;

use self::importer::SnapshotImporter;

/// Performs the importing of the genesis block from the snapshot.
pub async fn execute_genesis_block(
    watcher: StateWatcher,
    config: &Config,
    db: &CombinedDatabase,
) -> anyhow::Result<UncommittedImportResult<Changes>> {
    let genesis_block = create_genesis_block(config);
    tracing::info!("Genesis block created: {:?}", genesis_block.header());
    let db = db.clone().into_genesis();

    SnapshotImporter::import(
        db.clone(),
        genesis_block.clone(),
        config.snapshot_reader.clone(),
        watcher,
    )
    .await?;

    let genesis_progress_on_chain: Vec<String> = db
        .on_chain()
        .iter_all_keys::<GenesisMetadata<OnChain>>(None)
        .try_collect()?;
    let genesis_progress_off_chain: Vec<String> = db
        .off_chain()
        .iter_all_keys::<GenesisMetadata<OffChain>>(None)
        .try_collect()?;

    let chain_config = config.snapshot_reader.chain_config();
    let genesis = Genesis {
        chain_config_hash: chain_config.root()?.into(),
        coins_root: db.on_chain().genesis_coins_root()?.into(),
        messages_root: db.on_chain().genesis_messages_root()?.into(),
        contracts_root: db.on_chain().genesis_contracts_root()?.into(),
        transactions_root: db.on_chain().processed_transactions_root()?.into(),
    };

    let consensus = Consensus::Genesis(genesis);
    let block = SealedBlock {
        entity: genesis_block.clone(),
        consensus,
    };

    let mut database_transaction_off_chain = db.off_chain().clone().into_transaction();
    for key in genesis_progress_off_chain {
        database_transaction_off_chain
            .storage_as_mut::<GenesisMetadata<OffChain>>()
            .remove(&key)?;
    }
    database_transaction_off_chain.commit()?;

    let mut database_transaction_on_chain = db.on_chain().read_transaction();
    database_transaction_on_chain
        .storage_as_mut::<ConsensusParametersVersions>()
        .insert(
            &genesis_block.header().consensus_parameters_version,
            &chain_config.consensus_parameters,
        )?;

    let bytecode_root = Hasher::hash(chain_config.state_transition_bytecode.as_slice());
    database_transaction_on_chain
        .storage_as_mut::<StateTransitionBytecodeVersions>()
        .insert(
            &genesis_block.header().state_transition_bytecode_version,
            &bytecode_root,
        )?;
    database_transaction_on_chain
        .storage_as_mut::<UploadedBytecodes>()
        .insert(
            &bytecode_root,
            &UploadedBytecode::Completed(chain_config.state_transition_bytecode.clone()),
        )?;

    // Needs to be given the progress because `iter_all` is not implemented on db transactions.
    for key in genesis_progress_on_chain {
        database_transaction_on_chain
            .storage_as_mut::<GenesisMetadata<OnChain>>()
            .remove(&key)?;
    }

    let result = UncommittedImportResult::new(
        ImportResult::new_from_local(block, vec![], vec![]),
        database_transaction_on_chain.into_changes(),
    );

    Ok(result)
}

#[cfg(feature = "test-helpers")]
pub async fn execute_and_commit_genesis_block(
    config: &Config,
    db: &CombinedDatabase,
) -> anyhow::Result<()> {
    let result = execute_genesis_block(StateWatcher::default(), config, db).await?;
    let importer = fuel_core_importer::Importer::new(
        config
            .snapshot_reader
            .chain_config()
            .consensus_parameters
            .chain_id(),
        config.block_importer.clone(),
        db.on_chain().clone(),
        (),
        (),
    );
    importer.commit_result(result).await?;
    Ok(())
}

pub fn create_genesis_block(config: &Config) -> Block {
    let height;
    let da_height;
    let consensus_parameters_version;
    let state_transition_bytecode_version;
    let prev_root;

    // If the rollup continues the old rollup, the height of the new block should
    // be higher than that of the old chain by one to make it continuous.
    // The same applies to the state transition functions and consensus
    // parameters since it is a new chain.
    if let Some(latest_block) = config.snapshot_reader.last_block_config() {
        height = latest_block
            .block_height
            .succ()
            .expect("Block height overflow");
        consensus_parameters_version = latest_block
            .consensus_parameters_version
            .checked_add(1)
            .expect("Consensus parameters version overflow");
        state_transition_bytecode_version = latest_block
            .state_transition_version
            .checked_add(1)
            .expect("State transition bytecode version overflow");
        da_height = latest_block.da_block_height;
        prev_root = latest_block.blocks_root;
    } else {
        height = 0u32.into();
        #[cfg(feature = "relayer")]
        {
            da_height = config
                .relayer
                .as_ref()
                .map(|r| r.da_deploy_height)
                .unwrap_or_default();
        }
        #[cfg(not(feature = "relayer"))]
        {
            da_height = 0u64.into();
        }
        consensus_parameters_version = ConsensusParametersVersion::MIN;
        state_transition_bytecode_version = config
            .snapshot_reader
            .chain_config()
            .genesis_state_transition_version
            .unwrap_or(StateTransitionBytecodeVersion::MIN);
        prev_root = Bytes32::zeroed();
    }

    let transactions_ids = vec![];
    let message_ids = &[];
    let events = Default::default();
    Block::new(
        PartialBlockHeader {
            application: ApplicationHeader::<Empty> {
                da_height,
                consensus_parameters_version,
                state_transition_bytecode_version,
                generated: Empty,
            },
            consensus: ConsensusHeader::<Empty> {
                prev_root,
                height,
                time: fuel_core_types::tai64::Tai64::UNIX_EPOCH,
                generated: Empty,
            },
        },
        transactions_ids,
        message_ids,
        events,
    )
    .expect("The block is valid; qed")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        combined_database::CombinedDatabase,
        database::Database,
        service::{
            config::Config,
            FuelService,
        },
        ShutdownListener,
    };
    use fuel_core_chain_config::{
        BlobConfig,
        CoinConfig,
        ContractConfig,
        LastBlockConfig,
        MessageConfig,
        Randomize,
        StateConfig,
    };
    use fuel_core_producer::ports::BlockProducerDatabase;
    use fuel_core_storage::{
        tables::{
            Coins,
            ContractsAssets,
            ContractsState,
        },
        transactional::{
            AtomicView,
            HistoricalView,
        },
        StorageAsRef,
    };
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        entities::coins::coin::Coin,
        fuel_tx::UtxoId,
        fuel_types::{
            Address,
            AssetId,
            BlockHeight,
        },
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
    async fn config_initializes_block_height_of_genesis_block() {
        let block_height = BlockHeight::from(99u32);
        let service_config = Config::local_node_with_state_config(StateConfig {
            last_block: Some(LastBlockConfig {
                block_height,
                state_transition_version: 0,
                ..Default::default()
            }),
            ..Default::default()
        });

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        // The genesis block has next block height after the latest block of the previous network.
        let genesis_block_height = block_height.succ().unwrap();
        assert_eq!(
            genesis_block_height,
            db.latest_height()
                .expect("Expected a block height to be set")
        )
    }

    #[tokio::test]
    async fn genesis_columns_are_cleared_after_import() {
        let mut rng = StdRng::seed_from_u64(10);

        let coins = std::iter::repeat_with(|| CoinConfig {
            tx_pointer_block_height: 0.into(),
            ..Randomize::randomize(&mut rng)
        })
        .take(1000)
        .collect_vec();

        let messages = std::iter::repeat_with(|| MessageConfig {
            da_height: DaBlockHeight(0),
            ..MessageConfig::randomize(&mut rng)
        })
        .take(1000)
        .collect_vec();

        let blobs = std::iter::repeat_with(|| BlobConfig::randomize(&mut rng))
            .take(1000)
            .collect_vec();

        let contracts = std::iter::repeat_with(|| given_contract_config(&mut rng))
            .take(1000)
            .collect_vec();

        let state = StateConfig {
            coins,
            messages,
            blobs,
            contracts,
            last_block: Some(LastBlockConfig {
                block_height: BlockHeight::from(0u32),
                state_transition_version: 0,
                ..Default::default()
            }),
        };

        let service_config = Config::local_node_with_state_config(state);

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        let keys = db.iter_all::<GenesisMetadata<OnChain>>(None).count();
        assert_eq!(keys, 0);
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
            last_block: Some(LastBlockConfig {
                block_height: starting_height,
                state_transition_version: 0,
                ..Default::default()
            }),
            ..Default::default()
        };

        let service_config = Config::local_node_with_state_config(state);

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
        // given
        let mut rng = StdRng::seed_from_u64(10);

        let contract = given_contract_config(&mut rng);
        let contract_id = contract.contract_id;
        let states = contract.states.clone();

        let service_config = Config::local_node_with_state_config(StateConfig {
            contracts: vec![contract],
            ..Default::default()
        });
        let db = Database::default();

        // when
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        // then
        for state in states {
            let ret = db
                .storage::<ContractsState>()
                .get(&(&contract_id, &state.key).into())
                .unwrap()
                .expect("Expect a state entry to exist")
                .into_owned();
            assert_eq!(state.value, ret.0)
        }
    }

    #[cfg(feature = "test-helpers")]
    #[tokio::test]
    async fn tests_init_da_msgs() {
        use fuel_core_storage::tables::Messages;

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
        let config = Config::local_node_with_state_config(state);

        let db = CombinedDatabase::default();

        super::execute_and_commit_genesis_block(&config, &db)
            .await
            .unwrap();

        let expected_msg: fuel_core_types::entities::Message = msg.into();

        let ret_msg = db
            .on_chain()
            .storage::<Messages>()
            .get(expected_msg.id())
            .unwrap()
            .unwrap()
            .into_owned();

        assert_eq!(expected_msg, ret_msg);
    }

    #[tokio::test]
    async fn config_state_initializes_contract_balance() {
        let mut rng = StdRng::seed_from_u64(10);

        let contract = given_contract_config(&mut rng);
        let contract_id = contract.contract_id;
        let balances = contract.balances.clone();
        let service_config = Config::local_node_with_state_config(StateConfig {
            contracts: vec![contract],
            ..Default::default()
        });

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        for balance in balances {
            let ret = db
                .storage::<ContractsAssets>()
                .get(&(&contract_id, &balance.asset_id).into())
                .unwrap()
                .expect("Expected a balance to be present")
                .into_owned();

            assert_eq!(balance.amount, ret)
        }
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
            last_block: Some(LastBlockConfig {
                block_height: BlockHeight::from(9u32),
                state_transition_version: 0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let service_config = Config::local_node_with_state_config(state);

        let db = CombinedDatabase::default();
        let mut shutdown = ShutdownListener::spawn();
        let task = FuelService::new(db, service_config, &mut shutdown).unwrap();
        let init_result = task.start_and_await().await;

        assert!(init_result.is_err())
    }

    #[tokio::test]
    async fn contract_tx_pointer_cant_exceed_genesis_height() {
        let mut rng = StdRng::seed_from_u64(10);

        let state = StateConfig {
            contracts: vec![ContractConfig {
                // set txpointer height > genesis height
                tx_pointer_block_height: BlockHeight::from(11u32),
                ..given_contract_config(&mut rng)
            }],
            last_block: Some(LastBlockConfig {
                block_height: BlockHeight::from(9u32),
                state_transition_version: 0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let service_config = Config::local_node_with_state_config(state);

        let db = CombinedDatabase::default();
        let mut shutdown = ShutdownListener::spawn();
        let task = FuelService::new(db, service_config, &mut shutdown).unwrap();
        let init_result = task.start_and_await().await;

        assert!(init_result.is_err())
    }

    fn get_coins(db: &CombinedDatabase, owner: &Address) -> Vec<Coin> {
        db.off_chain()
            .latest_view()
            .unwrap()
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
        ContractConfig {
            tx_pointer_block_height: 0.into(),
            ..ContractConfig::randomize(rng)
        }
    }

    #[tokio::test]
    async fn config_state_initializes_contract() {
        let mut rng = StdRng::seed_from_u64(10);

        let initial_state = StateConfig {
            contracts: vec![given_contract_config(&mut rng)],
            ..Default::default()
        };
        let service_config = Config::local_node_with_state_config(initial_state.clone());

        let db = CombinedDatabase::default();
        FuelService::from_combined_database(db.clone(), service_config)
            .await
            .unwrap();

        let actual_state = db.read_state_config().unwrap();
        let mut expected_state = initial_state;
        let mut last_block = LastBlockConfig::default();
        let view = db.on_chain().latest_view().unwrap();
        last_block.block_height = view.latest_height().unwrap();
        last_block.state_transition_version = view
            .latest_block()
            .unwrap()
            .header()
            .state_transition_bytecode_version;
        last_block.blocks_root = view
            .block_header_merkle_root(&last_block.block_height)
            .unwrap();
        expected_state.last_block = Some(last_block);
        assert_eq!(expected_state, actual_state);
    }
}
