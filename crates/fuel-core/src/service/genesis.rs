use crate::{
    database::Database,
    service::{
        config::Config,
        FuelService,
    },
};
use anyhow::anyhow;
use fuel_core_chain_config::{
    ContractConfig,
    GenesisCommitment,
    StateConfig,
};
use fuel_core_executor::refs::ContractRef;
use fuel_core_poa::ports::BlockDb;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        FuelBlocks,
        Messages,
    },
    transactional::Transactional,
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
    },
    entities::{
        coin::{
            Coin,
            CoinStatus,
        },
        message::Message,
    },
    fuel_merkle::binary,
    fuel_tx::{
        Contract,
        MessageId,
        UtxoId,
    },
    fuel_types::{
        bytes::WORD_SIZE,
        Bytes32,
        ContractId,
    },
};
use itertools::Itertools;

impl FuelService {
    /// Loads state from the chain config into database
    pub(crate) fn initialize_state(
        config: &Config,
        database: &Database,
    ) -> anyhow::Result<()> {
        // check if chain is initialized
        if database.get_chain_name()?.is_none() {
            // start a db transaction for bulk-writing
            let mut import_tx = database.transaction();
            let database = import_tx.as_mut();

            Self::add_genesis_block(config, database)?;

            // Write transaction to db
            import_tx.commit()?;
        }

        Ok(())
    }

    pub fn add_genesis_block(
        config: &Config,
        database: &mut Database,
    ) -> anyhow::Result<()> {
        // Initialize the chain id and height.
        database.init(&config.chain_conf)?;

        let chain_config_hash = config.chain_conf.clone().root()?.into();
        let coins_root =
            Self::init_coin_state(database, &config.chain_conf.initial_state)?.into();
        let contracts_root =
            Self::init_contracts(database, &config.chain_conf.initial_state)?.into();
        let (messages_root, message_ids) =
            Self::init_da_messages(database, &config.chain_conf.initial_state)?;
        let messages_root = messages_root.into();

        let genesis = Genesis {
            chain_config_hash,
            coins_root,
            contracts_root,
            messages_root,
        };

        let block = Block::new(
            PartialBlockHeader {
                application: ApplicationHeader::<Empty> {
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
                metadata: None,
            },
            // Genesis block doesn't have any transaction.
            vec![],
            &message_ids,
        );

        let seal = Consensus::Genesis(genesis);
        let block_id = block.id();
        database
            .storage::<FuelBlocks>()
            .insert(&block_id.into(), &block.compress())?;
        database.seal_block(block_id, seal)
    }

    /// initialize coins
    pub fn init_coin_state(
        db: &mut Database,
        state: &Option<StateConfig>,
    ) -> anyhow::Result<MerkleRoot> {
        let mut coins_tree = binary::in_memory::MerkleTree::new();
        // TODO: Store merkle sum tree root over coins with unspecified utxo ids.
        let mut generated_output_index: u64 = 0;
        if let Some(state) = &state {
            if let Some(coins) = &state.coins {
                for coin in coins {
                    let utxo_id = UtxoId::new(
                        // generated transaction id([0..[out_index/255]])
                        coin.tx_id.unwrap_or_else(|| {
                            Bytes32::try_from(
                                (0..(Bytes32::LEN - WORD_SIZE))
                                    .map(|_| 0u8)
                                    .chain(
                                        (generated_output_index / 255)
                                            .to_be_bytes()
                                            .into_iter(),
                                    )
                                    .collect_vec()
                                    .as_slice(),
                            )
                            .expect("Incorrect genesis transaction id byte length")
                        }),
                        coin.output_index.map(|i| i as u8).unwrap_or_else(|| {
                            generated_output_index += 1;
                            (generated_output_index % 255) as u8
                        }),
                    );

                    let mut coin = Coin {
                        owner: coin.owner,
                        amount: coin.amount,
                        asset_id: coin.asset_id,
                        maturity: coin.maturity.unwrap_or_default(),
                        status: CoinStatus::Unspent,
                        block_created: coin.block_created.unwrap_or_default(),
                    };

                    if db.storage::<Coins>().insert(&utxo_id, &coin)?.is_some() {
                        return Err(anyhow!("Coin should not exist"))
                    }
                    coins_tree.push(coin.root()?.as_slice())
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
                for (generated_output_index, contract_config) in
                    contracts.iter().enumerate()
                {
                    let contract = Contract::from(contract_config.code.as_slice());
                    let salt = contract_config.salt;
                    let root = contract.root();
                    let contract_id =
                        contract.id(&salt, &root, &Contract::default_state_root());
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
                            &UtxoId::new(
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
                            ),
                        )?
                        .is_some()
                    {
                        return Err(anyhow!("Contract utxo should not exist"))
                    }
                    Self::init_contract_state(db, &contract_id, contract_config)?;
                    Self::init_contract_balance(db, &contract_id, contract_config)?;
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
            for (key, value) in contract_state {
                if db
                    .storage::<ContractsState>()
                    .insert(&(contract_id, key), value)?
                    .is_some()
                {
                    return Err(anyhow!("Contract state should not exist"))
                }
            }
        }
        Ok(())
    }

    fn init_da_messages(
        db: &mut Database,
        state: &Option<StateConfig>,
    ) -> anyhow::Result<(MerkleRoot, Vec<MessageId>)> {
        let mut message_tree = binary::in_memory::MerkleTree::new();
        let mut message_ids = vec![];
        if let Some(state) = &state {
            if let Some(message_state) = &state.messages {
                for msg in message_state {
                    let mut message = Message {
                        sender: msg.sender,
                        recipient: msg.recipient,
                        nonce: msg.nonce,
                        amount: msg.amount,
                        data: msg.data.clone(),
                        da_height: msg.da_height,
                        fuel_block_spend: None,
                    };

                    let message_id = message.id();
                    if db
                        .storage::<Messages>()
                        .insert(&message_id, &message)?
                        .is_some()
                    {
                        return Err(anyhow!("Message should not exist"))
                    }
                    message_tree.push(message.root()?.as_slice());
                    message_ids.push(message_id);
                }
            }
        }

        Ok((message_tree.root(), message_ids))
    }

    fn init_contract_balance(
        db: &mut Database,
        contract_id: &ContractId,
        contract: &ContractConfig,
    ) -> anyhow::Result<()> {
        // insert balances related to contract
        if let Some(balances) = &contract.balances {
            for (key, value) in balances {
                if db
                    .storage::<ContractsAssets>()
                    .insert(&(contract_id, key), value)?
                    .is_some()
                {
                    return Err(anyhow!("Contract balance should not exist"))
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::service::config::Config;
    use fuel_core_chain_config::{
        ChainConfig,
        CoinConfig,
        MessageConfig,
    };
    use fuel_core_storage::StorageAsRef;
    use fuel_core_types::{
        blockchain::primitives::{
            BlockHeight,
            DaBlockHeight,
        },
        fuel_asm::Opcode,
        fuel_types::{
            Address,
            AssetId,
            Salt,
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
            db.get_block_height()
                .unwrap()
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
        let alice_maturity = Some(rng.next_u32().into());
        let alice_block_created = Some(rng.next_u32().into());
        let alice_tx_id = Some(rng.gen());
        let alice_output_index = Some(rng.gen());
        let alice_utxo_id =
            UtxoId::new(alice_tx_id.unwrap(), alice_output_index.unwrap());

        // a coin with minimal options set
        let bob: Address = rng.gen();
        let asset_id_bob: AssetId = rng.gen();
        let bob_value = rng.gen();

        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    coins: Some(vec![
                        CoinConfig {
                            tx_id: alice_tx_id,
                            output_index: alice_output_index.map(|i| i as u64),
                            block_created: alice_block_created,
                            maturity: alice_maturity,
                            owner: alice,
                            amount: alice_value,
                            asset_id: asset_id_alice,
                        },
                        CoinConfig {
                            tx_id: None,
                            output_index: None,
                            block_created: None,
                            maturity: None,
                            owner: bob,
                            amount: bob_value,
                            asset_id: asset_id_bob,
                        },
                    ]),
                    height: alice_block_created.map(|h| {
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
        let bob_coins = get_coins(&db, &bob)
            .into_iter()
            .map(|(_, coin)| coin)
            .collect_vec();

        assert!(matches!(
            alice_coins.as_slice(),
            &[(utxo_id, Coin {
                owner,
                amount,
                asset_id,
                block_created,
                maturity,
                ..
            })] if utxo_id == alice_utxo_id
            && owner == alice
            && amount == alice_value
            && asset_id == asset_id_alice
            && block_created == alice_block_created.unwrap()
            && maturity == alice_maturity.unwrap(),
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
        let contract = Contract::from(Opcode::RET(0x10).to_bytes().to_vec());
        let root = contract.root();
        let id = contract.id(&salt, &root, &Contract::default_state_root());

        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    contracts: Some(vec![ContractConfig {
                        code: contract.into(),
                        salt,
                        state: Some(state),
                        balances: None,
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
            .get(&(&id, &test_key))
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

        FuelService::initialize_state(&config, db).unwrap();

        let expected_msg: Message = msg.into();

        let ret_msg = db
            .storage::<Messages>()
            .get(&expected_msg.id())
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
        let contract = Contract::from(Opcode::RET(0x10).to_bytes().to_vec());
        let root = contract.root();
        let id = contract.id(&salt, &root, &Contract::default_state_root());

        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    contracts: Some(vec![ContractConfig {
                        code: contract.into(),
                        salt,
                        state: None,
                        balances: Some(balances),
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
            .storage::<ContractsAssets<'_>>()
            .get(&(&id, &test_asset_id))
            .unwrap()
            .expect("Expected a balance to be present")
            .into_owned();

        assert_eq!(test_balance, ret)
    }

    fn get_coins(db: &Database, owner: &Address) -> Vec<(UtxoId, Coin)> {
        db.owned_coins_ids(owner, None, None)
            .map(|r| {
                let coin_id = r.unwrap();
                db.storage::<Coins>()
                    .get(&coin_id)
                    .map(|v| (coin_id, v.unwrap().into_owned()))
                    .unwrap()
            })
            .collect()
    }
}
