use crate::{
    database::Database,
    service::config::Config,
};
use anyhow::anyhow;
use fuel_core_chain_config::{
    CoinConfig,
    ContractConfig,
    GenesisCommitment,
    StateConfig,
};
use fuel_core_executor::refs::ContractRef;
use fuel_core_storage::{
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
    MerkleRoot,
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
        primitives::Empty,
        SealedBlock,
    },
    entities::{
        coins::coin::Coin,
        contract::ContractUtxoInfo,
        message::Message,
    },
    fuel_merkle::binary,
    fuel_tx::{
        Contract,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        bytes::WORD_SIZE,
        Bytes32,
        ContractId,
    },
    services::block_importer::{
        ImportResult,
        UncommittedResult as UncommittedImportResult,
    },
};
use itertools::Itertools;

pub mod off_chain;

/// Performs the importing of the genesis block from the snapshot.
pub fn execute_genesis_block(
    config: &Config,
    original_database: &Database,
) -> anyhow::Result<UncommittedImportResult<StorageTransaction<Database>>> {
    // start a db transaction for bulk-writing
    let mut database_transaction = Transactional::transaction(original_database);

    let database = database_transaction.as_mut();
    // Initialize the chain id and height.

    let chain_config_hash = config.chain_conf.root()?.into();
    let coins_root = init_coin_state(database, &config.chain_conf.initial_state)?.into();
    let contracts_root =
        init_contracts(database, &config.chain_conf.initial_state)?.into();
    let messages_root = init_da_messages(database, &config.chain_conf.initial_state)?;
    let messages_root = messages_root.into();

    let genesis = Genesis {
        chain_config_hash,
        coins_root,
        contracts_root,
        messages_root,
    };

    let block = create_genesis_block(config);
    let consensus = Consensus::Genesis(genesis);
    let block = SealedBlock {
        entity: block,
        consensus,
    };

    let result = UncommittedImportResult::new(
        ImportResult::new_from_local(block, vec![], vec![]),
        database_transaction,
    );
    Ok(result)
}

pub fn create_genesis_block(config: &Config) -> Block {
    let block = Block::new(
        PartialBlockHeader {
            application: ApplicationHeader::<Empty> {
                // TODO: Set `da_height` based on the chain config.
                da_height: Default::default(),
                generated: Empty,
            },
            consensus: ConsensusHeader::<Empty> {
                // The genesis is a first block, so previous root is zero.
                prev_root: Bytes32::zeroed(),
                // The initial height is defined by the `ChainConfig`.
                // If it is `None` then it will be zero.
                height: config
                    .chain_conf
                    .initial_state
                    .as_ref()
                    .map(|config| config.height.unwrap_or_else(|| 0u32.into()))
                    .unwrap_or_else(|| 0u32.into()),
                time: fuel_core_types::tai64::Tai64::UNIX_EPOCH,
                generated: Empty,
            },
        },
        // Genesis block doesn't have any transaction.
        vec![],
        &[],
    );
    block
}

#[cfg(feature = "test-helpers")]
pub async fn execute_and_commit_genesis_block(
    config: &Config,
    original_database: &Database,
) -> anyhow::Result<()> {
    let result = execute_genesis_block(config, original_database)?;
    let importer = fuel_core_importer::Importer::new(
        config.block_importer.clone(),
        original_database.clone(),
        (),
        (),
    );
    importer.commit_result(result).await?;
    Ok(())
}

fn init_coin_state(
    db: &mut Database,
    state: &Option<StateConfig>,
) -> anyhow::Result<MerkleRoot> {
    let mut coins_tree = binary::in_memory::MerkleTree::new();
    // TODO: Store merkle sum tree root over coins with unspecified utxo ids.
    let mut generated_output_index: u64 = 0;
    if let Some(state) = &state {
        if let Some(coins) = &state.coins {
            for coin in coins {
                let coin = create_coin_from_config(coin, &mut generated_output_index);
                let utxo_id = coin.utxo_id;
                let compressed_coin = coin.compress();
                // ensure coin can't point to blocks in the future
                if compressed_coin.tx_pointer().block_height()
                    > state.height.unwrap_or_default()
                {
                    return Err(anyhow!(
                        "coin tx_pointer height cannot be greater than genesis block"
                    ))
                }

                if db
                    .storage::<Coins>()
                    .insert(&utxo_id, &compressed_coin)?
                    .is_some()
                {
                    return Err(anyhow!("Coin should not exist"))
                }
                coins_tree.push(compressed_coin.root()?.as_slice())
            }
        }
    }
    Ok(coins_tree.root())
}

fn init_contracts(
    db: &mut Database,
    state: &Option<StateConfig>,
) -> anyhow::Result<MerkleRoot> {
    let mut contracts_tree = binary::in_memory::MerkleTree::new();
    // initialize contract state
    if let Some(state) = &state {
        if let Some(contracts) = &state.contracts {
            for (generated_output_index, contract_config) in contracts.iter().enumerate()
            {
                let contract = Contract::from(contract_config.code.as_slice());
                let salt = contract_config.salt;
                let root = contract.root();
                let contract_id = contract_config.contract_id;
                let utxo_id = if let (Some(tx_id), Some(output_idx)) =
                    (contract_config.tx_id, contract_config.output_index)
                {
                    UtxoId::new(tx_id, output_idx)
                } else {
                    #[allow(clippy::cast_possible_truncation)]
                    UtxoId::new(
                        // generated transaction id([0..[out_index/255]])
                        Bytes32::try_from(
                            (0..(Bytes32::LEN - WORD_SIZE))
                                .map(|_| 0u8)
                                .chain(
                                    (generated_output_index as u64 / 255)
                                        .to_be_bytes()
                                        .into_iter(),
                                )
                                .collect_vec()
                                .as_slice(),
                        )
                        .expect("Incorrect genesis transaction id byte length"),
                        generated_output_index as u8,
                    )
                };
                let tx_pointer = if let (Some(block_height), Some(tx_idx)) = (
                    contract_config.tx_pointer_block_height,
                    contract_config.tx_pointer_tx_idx,
                ) {
                    TxPointer::new(block_height, tx_idx)
                } else {
                    TxPointer::default()
                };

                if tx_pointer.block_height() > state.height.unwrap_or_default() {
                    return Err(anyhow!(
                        "contract tx_pointer cannot be greater than genesis block"
                    ))
                }

                // insert contract code
                if db
                    .storage::<ContractsRawCode>()
                    .insert(&contract_id, contract.as_ref())?
                    .is_some()
                {
                    return Err(anyhow!("Contract code should not exist"))
                }

                // insert contract root
                if db
                    .storage::<ContractsInfo>()
                    .insert(&contract_id, &(salt, root))?
                    .is_some()
                {
                    return Err(anyhow!("Contract info should not exist"))
                }
                if db
                    .storage::<ContractsLatestUtxo>()
                    .insert(
                        &contract_id,
                        &ContractUtxoInfo {
                            utxo_id,
                            tx_pointer,
                        },
                    )?
                    .is_some()
                {
                    return Err(anyhow!("Contract utxo should not exist"))
                }
                init_contract_state(db, &contract_id, contract_config)?;
                init_contract_balance(db, &contract_id, contract_config)?;
                contracts_tree
                    .push(ContractRef::new(&mut *db, contract_id).root()?.as_slice());
            }
        }
    }
    Ok(contracts_tree.root())
}

fn init_contract_state(
    db: &mut Database,
    contract_id: &ContractId,
    contract: &ContractConfig,
) -> anyhow::Result<()> {
    // insert state related to contract
    if let Some(contract_state) = &contract.state {
        db.init_contract_state(contract_id, contract_state.iter().map(Clone::clone))?;
    }
    Ok(())
}

fn init_da_messages(
    db: &mut Database,
    state: &Option<StateConfig>,
) -> anyhow::Result<MerkleRoot> {
    let mut message_tree = binary::in_memory::MerkleTree::new();
    if let Some(state) = &state {
        if let Some(message_state) = &state.messages {
            for msg in message_state {
                let message: Message = msg.clone().into();

                if db
                    .storage::<Messages>()
                    .insert(message.id(), &message)?
                    .is_some()
                {
                    return Err(anyhow!("Message should not exist"))
                }
                message_tree.push(message.root()?.as_slice());
            }
        }
    }

    Ok(message_tree.root())
}

fn init_contract_balance(
    db: &mut Database,
    contract_id: &ContractId,
    contract: &ContractConfig,
) -> anyhow::Result<()> {
    // insert balances related to contract
    if let Some(balances) = &contract.balances {
        db.init_contract_balances(contract_id, balances.clone().into_iter())?;
    }
    Ok(())
}

// TODO: Remove when re-genesis PRs are merged. Instead we will use `UtxoId` from the `CoinConfig`.
fn create_coin_from_config(coin: &CoinConfig, generated_output_index: &mut u64) -> Coin {
    let utxo_id = UtxoId::new(
        // generated transaction id([0..[out_index/255]])
        coin.tx_id.unwrap_or_else(|| {
            Bytes32::try_from(
                (0..(Bytes32::LEN - WORD_SIZE))
                    .map(|_| 0u8)
                    .chain(
                        (*generated_output_index / 255)
                            .to_be_bytes(),
                    )
                    .collect_vec()
                    .as_slice(),
            )
                .expect("Incorrect genesis transaction id byte length")
        }),
        coin.output_index.unwrap_or_else(|| {
            *generated_output_index = generated_output_index
                .checked_add(1)
                .expect("The maximum number of UTXOs supported in the genesis configuration has been exceeded.");
            (*generated_output_index % 255) as u8
        }),
    );

    Coin {
        utxo_id,
        owner: coin.owner,
        amount: coin.amount,
        asset_id: coin.asset_id,
        maturity: coin.maturity.unwrap_or_default(),
        tx_pointer: TxPointer::new(
            coin.tx_pointer_block_height.unwrap_or_default(),
            coin.tx_pointer_tx_idx.unwrap_or_default(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        combined_database::CombinedDatabase,
        service::{
            config::Config,
            FuelService,
            Task,
        },
    };
    use fuel_core_chain_config::{
        ChainConfig,
        CoinConfig,
        MessageConfig,
    };
    use fuel_core_services::RunnableService;
    use fuel_core_storage::{
        tables::{
            ContractsAssets,
            ContractsState,
        },
        StorageAsRef,
    };
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        entities::coins::coin::Coin,
        fuel_asm::op,
        fuel_types::{
            Address,
            AssetId,
            BlockHeight,
            Salt,
        },
    };
    use rand::{
        rngs::StdRng,
        Rng,
        RngCore,
        SeedableRng,
    };
    use std::vec;

    #[tokio::test]
    async fn config_initializes_block_height() {
        let test_height = BlockHeight::from(99u32);
        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    height: Some(test_height),
                    ..Default::default()
                }),
                ..ChainConfig::local_testnet()
            },
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        assert_eq!(
            test_height,
            db.latest_height()
                .expect("Expected a block height to be set")
        )
    }

    #[tokio::test]
    async fn config_state_initializes_multiple_coins_with_different_owners_and_asset_ids()
    {
        let mut rng = StdRng::seed_from_u64(10);

        // a coin with all options set
        let alice: Address = rng.gen();
        let asset_id_alice: AssetId = rng.gen();
        let alice_value = rng.gen();
        let alice_maturity: BlockHeight = rng.next_u32().into();
        let alice_block_created: BlockHeight = rng.next_u32().into();
        let alice_block_created_tx_idx = rng.gen();
        let alice_tx_id = rng.gen();
        let alice_output_index = rng.gen();
        let alice_utxo_id = UtxoId::new(alice_tx_id, alice_output_index);

        // a coin with minimal options set
        let bob: Address = rng.gen();
        let asset_id_bob: AssetId = rng.gen();
        let bob_value = rng.gen();

        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    coins: Some(vec![
                        CoinConfig {
                            tx_id: Some(alice_tx_id),
                            output_index: Some(alice_output_index),
                            tx_pointer_block_height: Some(alice_block_created),
                            tx_pointer_tx_idx: Some(alice_block_created_tx_idx),
                            maturity: Some(alice_maturity),
                            owner: alice,
                            amount: alice_value,
                            asset_id: asset_id_alice,
                        },
                        CoinConfig {
                            tx_id: None,
                            output_index: None,
                            tx_pointer_block_height: None,
                            tx_pointer_tx_idx: None,
                            maturity: None,
                            owner: bob,
                            amount: bob_value,
                            asset_id: asset_id_bob,
                        },
                    ]),
                    height: Some(alice_block_created).map(|h| {
                        let mut h: u32 = h.into();
                        // set starting height to something higher than alice's coin
                        h = h.saturating_add(rng.next_u32());
                        h.into()
                    }),
                    ..Default::default()
                }),
                ..ChainConfig::local_testnet()
            },
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
                maturity,
                ..
            }] if utxo_id == alice_utxo_id
            && owner == alice
            && amount == alice_value
            && asset_id == asset_id_alice
            && tx_pointer.block_height() == alice_block_created
            && maturity == alice_maturity,
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

        let test_key: Bytes32 = rng.gen();
        let test_value: Bytes32 = rng.gen();
        let state = vec![(test_key, test_value)];
        let salt: Salt = rng.gen();
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &Contract::default_state_root());

        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    contracts: Some(vec![ContractConfig {
                        contract_id,
                        code: contract.into(),
                        salt,
                        state: Some(state),
                        balances: None,
                        tx_id: Some(rng.gen()),
                        output_index: Some(rng.gen()),
                        tx_pointer_block_height: Some(0u32.into()),
                        tx_pointer_tx_idx: Some(rng.gen()),
                    }]),
                    ..Default::default()
                }),
                ..ChainConfig::local_testnet()
            },
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

        assert_eq!(test_value, ret)
    }

    #[tokio::test]
    async fn tests_init_da_msgs() {
        let mut rng = StdRng::seed_from_u64(32492);
        let mut config = Config::local_node();

        let msg = MessageConfig {
            sender: rng.gen(),
            recipient: rng.gen(),
            nonce: rng.gen(),
            amount: rng.gen(),
            data: vec![rng.gen()],
            da_height: DaBlockHeight(0),
        };

        config.chain_conf.initial_state = Some(StateConfig {
            messages: Some(vec![msg.clone()]),
            ..Default::default()
        });

        let db = &Database::default();

        let db_transaction = execute_genesis_block(&config, db)
            .unwrap()
            .into_transaction();

        let expected_msg: Message = msg.into();

        let ret_msg = db_transaction
            .as_ref()
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

        let test_asset_id: AssetId = rng.gen();
        let test_balance: u64 = rng.next_u64();
        let balances = vec![(test_asset_id, test_balance)];
        let salt: Salt = rng.gen();
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &Contract::default_state_root());

        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    contracts: Some(vec![ContractConfig {
                        contract_id,
                        code: contract.into(),
                        salt,
                        state: None,
                        balances: Some(balances),
                        tx_id: None,
                        output_index: None,
                        tx_pointer_block_height: None,
                        tx_pointer_tx_idx: None,
                    }]),
                    ..Default::default()
                }),
                ..ChainConfig::local_testnet()
            },
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
        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    height: Some(BlockHeight::from(10u32)),
                    coins: Some(vec![CoinConfig {
                        tx_id: None,
                        output_index: None,
                        // set txpointer height > genesis height
                        tx_pointer_block_height: Some(BlockHeight::from(11u32)),
                        tx_pointer_tx_idx: Some(0),
                        maturity: None,
                        owner: Default::default(),
                        amount: 10,
                        asset_id: Default::default(),
                    }]),
                    ..Default::default()
                }),
                ..ChainConfig::local_testnet()
            },
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
        let balances = vec![(test_asset_id, test_balance)];
        let salt: Salt = rng.gen();
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());

        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    height: Some(BlockHeight::from(10u32)),
                    contracts: Some(vec![ContractConfig {
                        contract_id: Default::default(),
                        code: contract.into(),
                        salt,
                        state: None,
                        balances: Some(balances),
                        tx_id: None,
                        output_index: None,
                        // set txpointer height > genesis height
                        tx_pointer_block_height: Some(BlockHeight::from(11u32)),
                        tx_pointer_tx_idx: Some(0),
                    }]),
                    ..Default::default()
                }),
                ..ChainConfig::local_testnet()
            },
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
}
