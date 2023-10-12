use crate::{
    database::{
        self,
        storage::{
            GenesisRootCalculator,
            GenesisRootCalculatorKey,
            StateImportCursor,
        },
        Database,
    },
    service::config::Config,
};
use anyhow::anyhow;
use fuel_core_chain_config::{
    CoinConfig,
    ContractComponent,
    ContractConfig,
    GenesisCommitment,
    MessageConfig,
};
use fuel_core_executor::refs::ContractRef;
use fuel_core_importer::Importer;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        FuelBlocks,
        Messages,
    },
    transactional::Transactional,
    MerkleRoot,
    StorageAsMut,
    StorageInspect,
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
        coins::coin::CompressedCoin,
        contract::ContractUtxoInfo,
        message::Message,
    },
    fuel_merkle::binary::{
        self,
        MerkleTree,
    },
    fuel_tx::{
        Contract,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        bytes::WORD_SIZE,
        AssetId,
        BlockHeight,
        Bytes32,
        ContractId,
    },
    services::block_importer::{
        ImportResult,
        UncommittedResult as UncommittedImportResult,
    },
};
use fuel_vm::fuel_merkle::binary::root_calculator::MerkleRootCalculator;
use itertools::Itertools;
use tracing::info;

/// Loads state from the chain config into database
pub fn maybe_initialize_state(
    config: &Config,
    database: &mut Database,
) -> anyhow::Result<()> {
    // check if chain is initialized

    if database.ids_of_latest_block()?.is_none() {
        import_chain_state(config, database)?;
        commit_genesis_block(config, database)?;
    }

    Ok(())
}

fn save_genesis_progress(
    cursor: usize,
    key: GenesisRootCalculatorKey,
    db: &mut Database,
) -> anyhow::Result<()> {
    original_database
        .storage::<StateImportCursor>()
        .insert(&(), &cursor)?;
    original_database
        .storage::<GenesisRootCalculator>()
        .insert(&key, &coins_tree)?;

    database_transaction.commit()
}

fn import_chain_state(
    config: &Config,
    original_database: &mut Database,
) -> anyhow::Result<()> {
    let block_height = config.chain_conf.block_height;

    let (mut cursor, mut coins_tree, mut messages_tree, mut contracts_tree) =
        resume_import(original_database)?;

    let state_importer = config.state_importer.clone();

    state_importer.try_for_each(|coin| {
        let mut database_transaction = Transactional::transaction(original_database);
        let database = database_transaction.as_mut();

        let root = init_coin(database, coin?, cursor as u64, block_height)?;
        coins_tree.push(root.as_slice());

        save_genesis_progress(
            cursor + 1,
            GenesisRootCalculatorKey::Coins,
            original_database,
        )?
    })?;

    let message_importer = state_importer.messages();
    message_importer.try_for_each(|message| {
        let mut database_transaction = Transactional::transaction(original_database);
        let database = database_transaction.as_mut();

        let root = init_da_message(database, message)?;
        messages_tree.push(root.as_slice());

        save_genesis_progress(
            cursor + 1,
            GenesisRootCalculatorKey::Messages,
            original_database,
        )?
    })?;

    let contract_importer = message_importer.contracts();
    contract_importer.try_for_each(|message| {
        match contract? {
            // TODO output index
            ContractComponent::ContractMetadata(contract) => {
                init_contract(original_database, contract, cursor as u64, block_height)?
            }
            ContractComponent::ContractState(contract_id, key, value) => {
                init_contract_state(original_database, &contract_id, key, value)?
            }
            ContractComponent::ContractAsset(contract_id, asset_id, balance) => {
                init_contract_balance(
                    original_database,
                    &contract_id,
                    AssetId::from(*asset_id),
                    balance,
                )?;

                // State file specs guarantee that ContractAsset will come last when reading contract state
                // We can calculate the root at this point
                contracts_tree.push(
                    ContractRef::new(&mut *original_database, contract_id)
                        .root()?
                        .as_slice(),
                );
                save_genesis_progress(
                    cursor + 1,
                    GenesisRootCalculatorKey::Contracts,
                    original_database,
                )?
            }
        }
    })?;

    Ok(())
}

fn resume_import(
    mut database: &Database,
) -> anyhow::Result<(
    usize,
    MerkleRootCalculator,
    MerkleRootCalculator,
    MerkleRootCalculator,
)> {
    let cursor = *database
        .storage::<StateImportCursor>()
        .get(&())?
        .unwrap_or_default();

    Ok((
        cursor,
        MerkleRootCalculator::default(),
        MerkleRootCalculator::default(),
        MerkleRootCalculator::default(),
    ))
}

fn commit_genesis_block(
    config: &Config,
    original_database: &Database,
) -> anyhow::Result<()> {
    let mut database_transaction = Transactional::transaction(original_database);
    let database = database_transaction.as_mut();

    // TODO: load roots from storage
    let chain_config_hash = config.chain_conf.root()?.into();
    let genesis = Genesis {
        chain_config_hash,
        coins_root: binary::in_memory::MerkleTree::new().root().into(),
        contracts_root: binary::in_memory::MerkleTree::new().root().into(),
        messages_root: binary::in_memory::MerkleTree::new().root().into(),
    };

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
                height: config.chain_conf.block_height,
                time: fuel_core_types::tai64::Tai64::UNIX_EPOCH,
                generated: Empty,
            },
        },
        // Genesis block doesn't have any transaction.
        vec![],
        &[],
    );

    let block_id = block.id();
    database.storage::<FuelBlocks>().insert(
        &block_id,
        &block.compress(&config.chain_conf.consensus_parameters.chain_id),
    )?;
    let consensus = Consensus::Genesis(genesis);
    let block = SealedBlock {
        entity: block,
        consensus,
    };

    let importer = Importer::new(
        config.block_importer.clone(),
        original_database.clone(),
        (),
        (),
    );
    importer.commit_result(UncommittedImportResult::new(
        ImportResult::new_from_local(block, vec![]),
        database_transaction,
    ))?;

    Ok(())
}

fn init_coin(
    db: &mut Database,
    coin: CoinConfig,
    output_index: u64,
    height: BlockHeight,
) -> anyhow::Result<MerkleRoot> {
    // TODO: Store merkle sum tree root over coins with unspecified utxo ids.
    let utxo_id = UtxoId::new(
        // generated transaction id([0..[out_index/255]])
        coin.tx_id.unwrap_or_else(|| {
            Bytes32::try_from(
                (0..(Bytes32::LEN - WORD_SIZE))
                    .map(|_| 0u8)
                    .chain((output_index / 255).to_be_bytes().into_iter())
                    .collect_vec()
                    .as_slice(),
            )
            .expect("Incorrect genesis transaction id byte length")
        }),
        coin.output_index
            .unwrap_or_else(|| (output_index % 255) as u8),
    );

    let coin = CompressedCoin {
        owner: coin.owner,
        amount: coin.amount,
        asset_id: coin.asset_id,
        maturity: coin.maturity.unwrap_or_default(),
        tx_pointer: TxPointer::new(
            coin.tx_pointer_block_height.unwrap_or_default(),
            coin.tx_pointer_tx_idx.unwrap_or_default(),
        ),
    };

    // ensure coin can't point to blocks in the future
    if coin.tx_pointer.block_height() > height {
        return Err(anyhow!(
            "coin tx_pointer height cannot be greater than genesis block"
        ))
    }

    if db.storage::<Coins>().insert(&utxo_id, &coin)?.is_some() {
        return Err(anyhow!("Coin should not exist"))
    }
    coin.root()
}

fn init_da_message(db: &mut Database, msg: MessageConfig) -> anyhow::Result<MerkleRoot> {
    let message = Message {
        sender: msg.sender,
        recipient: msg.recipient,
        nonce: msg.nonce,
        amount: msg.amount,
        data: msg.data.clone(),
        da_height: msg.da_height,
    };

    if db
        .storage::<Messages>()
        .insert(message.id(), &message)?
        .is_some()
    {
        return Err(anyhow!("Message should not exist"))
    }

    message.root()
}

fn init_contract(
    db: &mut Database,
    contract_config: ContractConfig,
    output_index: u64,
    height: BlockHeight,
) -> anyhow::Result<()> {
    // initialize contract state
    let contract = Contract::from(contract_config.code.as_slice());
    let salt = contract_config.salt;
    let root = contract.root();
    let contract_id = contract_config.contract_id;
    let utxo_id = if let (Some(tx_id), Some(output_idx)) =
        (contract_config.tx_id, contract_config.output_index)
    {
        UtxoId::new(tx_id, output_idx)
    } else {
        UtxoId::new(
            // generated transaction id([0..[out_index/255]])
            Bytes32::try_from(
                (0..(Bytes32::LEN - WORD_SIZE))
                    .map(|_| 0u8)
                    .chain((output_index as u64 / 255).to_be_bytes().into_iter())
                    .collect_vec()
                    .as_slice(),
            )
            .expect("Incorrect genesis transaction id byte length"),
            output_index as u8,
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

    if tx_pointer.block_height() > height {
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
    Ok(())
}

fn init_contract_state(
    db: &mut Database,
    contract_id: &ContractId,
    key: Bytes32,
    value: Bytes32,
) -> anyhow::Result<()> {
    // insert state related to contract

    // TODO: need to insert piecewise
    // if let Some(contract_state) = &contract.state {
    //    db.init_contract_state(contract_id, contract_state.iter().map(Clone::clone))?;
    // }

    Ok(())
}

fn init_contract_balance(
    db: &mut Database,
    contract_id: &ContractId,
    asset_id: AssetId,
    balance: u64,
) -> anyhow::Result<()> {
    // insert balances related to contract

    // TODO: need to insert piecewise
    // if let Some(balances) = &contract.balances {
    //    db.init_contract_balances(contract_id, balances.clone().into_iter())?;
    // }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::service::{
        config::Config,
        FuelService,
    };
    use fuel_core_chain_config::{
        ChainConfig,
        CoinConfig,
        MessageConfig,
    };
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
    async fn config_initializes_chain_name() {
        let test_name = "test_net_123".to_string();
        let service_config = Config {
            chain_conf: ChainConfig {
                chain_name: test_name.clone(),
                ..ChainConfig::local_testnet()
            },
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        assert_eq!(
            test_name,
            db.get_chain_name()
                .unwrap()
                .expect("Expected a chain name to be set")
        )
    }

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

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
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

        maybe_initialize_state(&config, db).unwrap();

        let expected_msg: Message = msg.into();

        let ret_msg = db
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

        let db = Database::default();
        let init_result = FuelService::from_database(db.clone(), service_config).await;

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

        let db = Database::default();
        let init_result = FuelService::from_database(db.clone(), service_config).await;

        assert!(init_result.is_err())
    }

    fn get_coins(db: &Database, owner: &Address) -> Vec<Coin> {
        db.owned_coins_ids(owner, None, None)
            .map(|r| {
                let coin_id = r.unwrap();
                db.storage::<Coins>()
                    .get(&coin_id)
                    .map(|v| v.unwrap().into_owned().uncompress(coin_id))
                    .unwrap()
            })
            .collect()
    }
}
