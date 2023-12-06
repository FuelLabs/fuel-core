use crate::{
    database::Database,
    service::config::Config,
};
use anyhow::anyhow;
use fuel_core_chain_config::{
    CoinConfig,
    ContractBalanceConfig,
    ContractConfig,
    ContractStateConfig,
    GenesisCommitment,
    IntoIter,
    MessageConfig,
};

use fuel_core_executor::refs::ContractRef;
use fuel_core_importer::Importer;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState as TableContractsState,
        FuelBlocks,
        Messages,
    },
    transactional::Transactional,
    MerkleRoot,
    StorageAsMut,
    StorageMutate,
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
    fuel_merkle::{
        binary,
        sparse::{
            in_memory::MerkleTree,
            MerkleTreeKey,
        },
    },
    fuel_tx::{
        Contract,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        bytes::WORD_SIZE,
        BlockHeight,
        Bytes32,
        ContractId,
    },
    services::block_importer::{
        ImportResult,
        UncommittedResult as UncommittedImportResult,
    },
};
use itertools::Itertools;

/// Loads state from the chain config into database
pub fn maybe_initialize_state(
    config: &Config,
    database: &Database,
) -> anyhow::Result<()> {
    // check if chain is initialized
    if database.ids_of_latest_block()?.is_none() {
        let roots = import_chain_state(config, database)?;
        commit_genesis_block(config, database, roots)?;
    }

    Ok(())
}

struct StateRoots {
    messages: Bytes32,
    coins: Bytes32,
    contracts: Bytes32,
}

fn import_chain_state(
    config: &Config,
    original_database: &Database,
) -> anyhow::Result<StateRoots> {
    let block_height = config.chain_config.height.unwrap_or_default();

    let coins_root = {
        let coins = config.state_reader.coins()?;
        let roots = import_coin_configs(&original_database, coins, block_height)?
            .into_iter()
            .sorted()
            .enumerate()
            .map(|(index, coin)| (MerkleTreeKey::new(index.to_be_bytes()), coin));
        let (root, _) = MerkleTree::nodes_from_set(roots);
        root
    };

    let messages_root = {
        let messages = config.state_reader.messages()?;
        let roots = import_message_configs(&original_database, messages)?
            .into_iter()
            .sorted()
            .enumerate()
            .map(|(index, message)| (MerkleTreeKey::new(index.to_be_bytes()), message));
        let (root, _) = MerkleTree::nodes_from_set(roots);
        root
    };

    let contracts = config.state_reader.contracts()?;
    let mut contract_ids =
        import_contract_configs(&original_database, contracts, block_height)?;
    contract_ids.sort();

    let contract_states = config.state_reader.contract_state()?;
    import_contract_state(&original_database, contract_states)?;

    let contract_balances = config.state_reader.contract_balance()?;
    import_contract_balance(&original_database, contract_balances)?;

    let mut contracts_tree = binary::in_memory::MerkleTree::new();
    for contract_id in contract_ids {
        // TODO
        let mut database_transaction = Transactional::transaction(original_database);
        let database = database_transaction.as_mut();
        contracts_tree.push(ContractRef::new(database, contract_id).root()?.as_slice());
    }

    let contracts_root = contracts_tree.root();

    Ok(StateRoots {
        messages: messages_root.into(),
        coins: coins_root.into(),
        contracts: contracts_root.into(),
    })
}

fn commit_genesis_block(
    config: &Config,
    original_database: &Database,
    roots: StateRoots,
) -> anyhow::Result<()> {
    let mut database_transaction = Transactional::transaction(original_database);
    let database = database_transaction.as_mut();

    // TODO: load roots from storage
    let chain_config_hash = config.chain_config.root()?.into();

    let genesis = Genesis {
        chain_config_hash,
        coins_root: roots.coins,
        contracts_root: roots.contracts,
        messages_root: roots.messages,
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
                height: config.chain_config.height.unwrap_or_default(),
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
        &block.compress(&config.chain_config.consensus_parameters.chain_id),
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

fn import_coin_configs(
    database: &Database,
    coins: IntoIter<CoinConfig>,
    block_height: BlockHeight,
) -> anyhow::Result<Vec<Bytes32>> {
    // let (cursor, root_calculator) =
    // resume_import(database, StateImportProgressKey::Coins)?;
    // let mut state_reader = JsonBatchReader::new(coins_reader, cursor)?;
    let mut roots = vec![];
    let mut generated_output_idx = 0;

    for batch in coins {
        let mut database_transaction = Transactional::transaction(database);
        let database = database_transaction.as_mut();

        // TODO
        let batch = batch.unwrap();

        // TODO: set output_index
        batch.data.iter().try_for_each(|coin| {
            let root  = init_coin(database, coin, generated_output_idx, block_height)?;
            roots.push(root.into());

            generated_output_idx = generated_output_idx
                .checked_add(1)
                .expect("The maximum number of UTXOs supported in the genesis configuration has been exceeded.");

            /*
            save_import_progress(
                database,
                StateImportProgressKey::Coins,
                cursor + 1, // TODO: advance by # bytes read
                root_calculator,
            ) */
            Ok::<(), anyhow::Error>(())
        })?;

        database_transaction.commit()?;
    }

    Ok(roots)
}

fn import_message_configs(
    database: &Database,
    messages: IntoIter<MessageConfig>,
) -> anyhow::Result<Vec<Bytes32>> {
    // let (cursor, root_calculator) =
    // resume_import(database, StateImportProgressKey::Coins)?;
    // let mut state_reader = JsonBatchReader::new(coins_reader, cursor)?;
    let mut roots = vec![];

    for batch in messages {
        let mut database_transaction = Transactional::transaction(database);
        let database = database_transaction.as_mut();

        // TODO
        let batch = batch.unwrap();

        // TODO: set output_index
        batch.data.iter().try_for_each(|message| {
            let root = init_da_message(database, message)?;
            roots.push(root.into());

            // save_import_progress(
            // database,
            // StateImportProgressKey::Coins,
            // cursor + 1, // TODO: advance by # bytes read
            // root_calculator,
            // )
            Ok::<(), anyhow::Error>(())
        })?;

        database_transaction.commit()?;
    }

    Ok(roots)
}

fn import_contract_configs(
    database: &Database,
    contracts: IntoIter<ContractConfig>,
    block_height: BlockHeight,
) -> anyhow::Result<Vec<ContractId>> {
    // let (cursor, root_calculator) =
    // resume_import(database, StateImportProgressKey::Coins)?;
    // let mut state_reader = JsonBatchReader::new(coins_reader, cursor)?;
    let mut generated_output_idx = 0;

    let mut contract_ids = vec![];

    for batch in contracts {
        let mut database_transaction = Transactional::transaction(database);
        let database = database_transaction.as_mut();

        // TODO
        let batch = batch.unwrap();

        // TODO: set output_index
        batch.data.iter().try_for_each(|contract| {
            contract_ids.push(contract.contract_id);
            init_contract(database, contract, generated_output_idx, block_height)?;

            generated_output_idx = generated_output_idx
                .checked_add(1)
                .expect("The maximum number of UTXOs supported in the genesis configuration has been exceeded.");

            /*
            save_import_progress(
                database,
                StateImportProgressKey::Coins,
                cursor + 1, // TODO: advance by # bytes read
                root_calculator,
            ) */
            Ok::<(), anyhow::Error>(())
        })?;

        database_transaction.commit()?;
    }

    Ok(contract_ids)
}

fn import_contract_state(
    database: &Database,
    contract_states: IntoIter<ContractStateConfig>,
) -> anyhow::Result<()> {
    // let (cursor, root_calculator) =
    // resume_import(database, StateImportProgressKey::Coins)?;
    // let mut state_reader = JsonBatchReader::new(coins_reader, cursor)?;

    for batch in contract_states {
        let mut database_transaction = Transactional::transaction(database);
        let database = database_transaction.as_mut();

        // TODO
        let batch = batch.unwrap();

        // TODO: set output_index
        batch.data.iter().try_for_each(|state| {
            init_contract_state(database, state)?;
            // save_import_progress(
            // database,
            // StateImportProgressKey::Coins,
            // cursor + 1, // TODO: advance by # bytes read
            // root_calculator,
            // )
            Ok::<(), anyhow::Error>(())
        })?;

        database_transaction.commit()?;
    }

    Ok(())
}

fn import_contract_balance(
    database: &Database,
    contract_states: IntoIter<ContractBalanceConfig>,
) -> anyhow::Result<()> {
    // let (cursor, root_calculator) =
    // resume_import(database, StateImportProgressKey::Coins)?;
    // let mut state_reader = JsonBatchReader::new(coins_reader, cursor)?;

    for batch in contract_states {
        let mut database_transaction = Transactional::transaction(database);
        let database = database_transaction.as_mut();

        // TODO
        let batch = batch.unwrap();

        // TODO: set output_index
        batch.data.iter().try_for_each(|balance| {
            init_contract_balance(database, balance)?;
            // save_import_progress(
            // database,
            // StateImportProgressKey::Coins,
            // cursor + 1, // TODO: advance by # bytes read
            // root_calculator,
            // )
            Ok::<(), anyhow::Error>(())
        })?;

        database_transaction.commit()?;
    }

    Ok(())
}

fn init_coin(
    db: &mut Database,
    coin: &CoinConfig,
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
    let coin_height = coin.tx_pointer.block_height();
    if coin_height > height {
        return Err(anyhow!(
            "coin tx_pointer height ({coin_height}) cannot be greater than genesis block ({height})"
        ));
    }

    if db.storage::<Coins>().insert(&utxo_id, &coin)?.is_some() {
        return Err(anyhow!("Coin should not exist"));
    }
    coin.root()
}

fn init_contract(
    db: &mut Database,
    contract_config: &ContractConfig,
    output_index: u64,
    height: BlockHeight,
) -> anyhow::Result<()> {
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

    // insert contract root
    if db
        .storage::<ContractsInfo>()
        .insert(&contract_id, &(salt, root))?
        .is_some()
    {
        return Err(anyhow!("Contract info should not exist"));
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
        return Err(anyhow!("Contract utxo should not exist"));
    }
    Ok(())
}

fn init_contract_state(
    db: &mut Database,
    state: &ContractStateConfig,
) -> anyhow::Result<()> {
    let key = (&ContractId::from(*state.contract_id), &state.key).into();
    StorageMutate::<TableContractsState>::insert(db, &key, &state.value)?;

    Ok(())
}

fn init_contract_balance(
    db: &mut Database,
    balance: &ContractBalanceConfig,
) -> anyhow::Result<()> {
    let key = (&ContractId::from(*balance.contract_id), &balance.asset_id).into();
    StorageMutate::<ContractsAssets>::insert(db, &key, &balance.amount).unwrap();

    Ok(())
}

fn init_da_message(db: &mut Database, msg: &MessageConfig) -> anyhow::Result<MerkleRoot> {
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
        return Err(anyhow!("Message should not exist"));
    }

    message.root()
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
        StateConfig,
        StateReader,
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
            chain_config: ChainConfig {
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
        let height = BlockHeight::from(99u32);
        let service_config = Config {
            chain_config: ChainConfig {
                height: Some(height),
                ..ChainConfig::local_testnet()
            },
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        assert_eq!(
            height,
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

        let state = StateConfig {
            coins: vec![
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
            ],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state, 1);

        let starting_height = {
            let mut h: u32 = alice_block_created.into();
            h = h.saturating_add(rng.next_u32());
            Some(h.into())
        };
        let service_config = Config {
            chain_config: ChainConfig {
                height: starting_height,
                ..ChainConfig::local_testnet()
            },
            state_reader,
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

        let salt: Salt = rng.gen();
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &Contract::default_state_root());

        let test_key = rng.gen();
        let test_value = rng.gen();
        let contract_state = ContractStateConfig {
            contract_id: Bytes32::from(*contract_id),
            key: test_key,
            value: test_value,
        };

        let state = StateConfig {
            contracts: vec![ContractConfig {
                contract_id,
                code: contract.into(),
                salt,
                state: None,
                balances: None,
                tx_id: Some(rng.gen()),
                output_index: Some(rng.gen()),
                tx_pointer_block_height: Some(0u32.into()),
                tx_pointer_tx_idx: Some(rng.gen()),
            }],
            contract_state: vec![contract_state],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state, 1);

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

        assert_eq!(test_value, ret)
    }

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
        let state_reader = StateReader::in_memory(state, 1);

        let config = Config {
            state_reader,
            ..Config::local_node()
        };

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

        let salt: Salt = rng.gen();
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &Contract::default_state_root());

        let test_asset_id = rng.gen();
        let test_balance = rng.next_u64();
        let contract_balance = ContractBalanceConfig {
            contract_id: Bytes32::from(*contract_id),
            asset_id: test_asset_id,
            amount: test_balance,
        };

        let state = StateConfig {
            contracts: vec![ContractConfig {
                contract_id,
                code: contract.into(),
                salt,
                state: None,
                balances: None,
                tx_id: None,
                output_index: None,
                tx_pointer_block_height: None,
                tx_pointer_tx_idx: None,
            }],
            contract_balance: vec![contract_balance],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state, 1);

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
                tx_id: None,
                output_index: None,
                // set txpointer height > genesis height
                tx_pointer_block_height: Some(BlockHeight::from(11u32)),
                tx_pointer_tx_idx: Some(0),
                maturity: None,
                owner: Default::default(),
                amount: 10,
                asset_id: Default::default(),
            }],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state, 1);

        let service_config = Config {
            chain_config: ChainConfig {
                height: Some(BlockHeight::from(10u32)),
                ..ChainConfig::local_testnet()
            },
            state_reader,
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

        let state = StateConfig {
            contracts: vec![ContractConfig {
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
            }],
            ..Default::default()
        };
        let state_reader = StateReader::in_memory(state, 1);

        let service_config = Config {
            chain_config: ChainConfig {
                height: Some(BlockHeight::from(10u32)),
                ..ChainConfig::local_testnet()
            },
            state_reader,
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
