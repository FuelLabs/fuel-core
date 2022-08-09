use crate::{
    config::{
        chain_config::{ContractConfig, StateConfig},
        Config,
    },
    database::Database,
    service::FuelService,
};
use anyhow::Result;
use fuel_core_interfaces::model::Message;
use fuel_core_interfaces::{
    common::{
        fuel_storage::{MerkleStorage, Storage},
        fuel_tx::{Contract, MessageId, UtxoId},
        fuel_types::{bytes::WORD_SIZE, AssetId, Bytes32, ContractId, Salt, Word},
    },
    model::{Coin, CoinStatus},
};
use itertools::Itertools;

impl FuelService {
    /// Loads state from the chain config into database
    pub(crate) fn initialize_state(config: &Config, database: &Database) -> Result<()> {
        // start a db transaction for bulk-writing
        let mut import_tx = database.transaction();
        let database = import_tx.as_mut();

        // check if chain is initialized
        if database.get_chain_name()?.is_none() {
            // initialize the chain id
            database.init(config)?;

            if let Some(initial_state) = &config.chain_conf.initial_state {
                Self::init_coin_state(database, initial_state)?;
                Self::init_contracts(database, initial_state)?;
                Self::init_da_messages(database, initial_state)?;
            }
        }

        // Write transaction to db
        import_tx.commit()?;

        Ok(())
    }

    /// initialize coins
    fn init_coin_state(db: &mut Database, state: &StateConfig) -> Result<()> {
        // TODO: Store merkle sum tree root over coins with unspecified utxo ids.
        let mut generated_output_index: u64 = 0;
        if let Some(coins) = &state.coins {
            for coin in coins {
                let utxo_id = UtxoId::new(
                    // generated transaction id([0..[out_index/255]])
                    coin.tx_id.unwrap_or_else(|| {
                        Bytes32::try_from(
                            (0..(Bytes32::LEN - WORD_SIZE))
                                .map(|_| 0u8)
                                .chain((generated_output_index / 255).to_be_bytes().into_iter())
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

                let coin = Coin {
                    owner: coin.owner,
                    amount: coin.amount,
                    asset_id: coin.asset_id,
                    maturity: coin.maturity.unwrap_or_default(),
                    status: CoinStatus::Unspent,
                    block_created: coin.block_created.unwrap_or_default(),
                };

                let _ = Storage::<UtxoId, Coin>::insert(db, &utxo_id, &coin)?;
            }
        }
        Ok(())
    }

    fn init_contracts(db: &mut Database, state: &StateConfig) -> Result<()> {
        // initialize contract state
        if let Some(contracts) = &state.contracts {
            for (generated_output_index, contract_config) in contracts.iter().enumerate() {
                let contract = Contract::from(contract_config.code.as_slice());
                let salt = contract_config.salt;
                let root = contract.root();
                let contract_id = contract.id(&salt, &root, &Contract::default_state_root());
                // insert contract code
                let _ = Storage::<ContractId, Contract>::insert(db, &contract_id, &contract)?;
                // insert contract root
                let _ = Storage::<ContractId, (Salt, Bytes32)>::insert(
                    db,
                    &contract_id,
                    &(salt, root),
                )?;
                let _ = Storage::<ContractId, UtxoId>::insert(
                    db,
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
                )?;
                Self::init_contract_state(db, &contract_id, contract_config)?;
                Self::init_contract_balance(db, &contract_id, contract_config)?;
            }
        }
        Ok(())
    }

    fn init_contract_state(
        db: &mut Database,
        contract_id: &ContractId,
        contract: &ContractConfig,
    ) -> Result<()> {
        // insert state related to contract
        if let Some(contract_state) = &contract.state {
            for (key, value) in contract_state {
                MerkleStorage::<ContractId, Bytes32, Bytes32>::insert(db, contract_id, key, value)?;
            }
        }
        Ok(())
    }

    fn init_da_messages(db: &mut Database, state: &StateConfig) -> Result<()> {
        if let Some(message_state) = &state.messages {
            for msg in message_state {
                let message = Message {
                    sender: msg.sender,
                    recipient: msg.recipient,
                    owner: msg.owner,
                    nonce: msg.nonce,
                    amount: msg.amount,
                    data: msg.data.clone(),
                    da_height: msg.da_height,
                    fuel_block_spend: None,
                };

                Storage::<MessageId, Message>::insert(db, &message.id(), &message)?;
            }
        }

        Ok(())
    }

    fn init_contract_balance(
        db: &mut Database,
        contract_id: &ContractId,
        contract: &ContractConfig,
    ) -> Result<()> {
        // insert balances related to contract
        if let Some(balances) = &contract.balances {
            for (key, value) in balances {
                MerkleStorage::<ContractId, AssetId, Word>::insert(db, contract_id, key, value)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use crate::config::chain_config::{
        ChainConfig, CoinConfig, ContractConfig, MessageConfig, StateConfig,
    };
    use crate::config::Config;
    use crate::model::BlockHeight;
    use fuel_core_interfaces::common::{
        fuel_asm::Opcode,
        fuel_types::{Address, AssetId, Word},
    };
    use fuel_core_interfaces::model::Message;
    use itertools::Itertools;
    use rand::{rngs::StdRng, Rng, RngCore, SeedableRng};

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
    async fn config_state_initializes_multiple_coins_with_different_owners_and_asset_ids() {
        let mut rng = StdRng::seed_from_u64(10);

        // a coin with all options set
        let alice: Address = rng.gen();
        let asset_id_alice: AssetId = rng.gen();
        let alice_value = rng.gen();
        let alice_maturity = Some(rng.next_u32().into());
        let alice_block_created = Some(rng.next_u32().into());
        let alice_tx_id = Some(rng.gen());
        let alice_output_index = Some(rng.gen());
        let alice_utxo_id = UtxoId::new(alice_tx_id.unwrap(), alice_output_index.unwrap());

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

        let alice_coins = get_coins(&db, alice);
        let bob_coins = get_coins(&db, bob)
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

        let ret = MerkleStorage::<ContractId, Bytes32, Bytes32>::get(&db, &id, &test_key)
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
            owner: rng.gen(),
            nonce: rng.gen(),
            amount: rng.gen(),
            data: vec![rng.gen()],
            da_height: 0,
        };

        config.chain_conf.initial_state = Some(StateConfig {
            messages: Some(vec![msg.clone()]),
            ..Default::default()
        });

        let db = Database::default();

        FuelService::initialize_state(&config, &db).unwrap();

        let expected_msg: Message = msg.into();

        let ret_msg = Storage::<MessageId, Message>::get(&db, &expected_msg.id())
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

        let ret = MerkleStorage::<ContractId, AssetId, Word>::get(&db, &id, &test_asset_id)
            .unwrap()
            .expect("Expected a balance to be present")
            .into_owned();

        assert_eq!(test_balance, ret)
    }

    fn get_coins(db: &Database, owner: Address) -> Vec<(UtxoId, Coin)> {
        db.owned_coins(owner, None, None)
            .map(|r| {
                r.and_then(|coin_id| {
                    Storage::<UtxoId, Coin>::get(db, &coin_id)
                        .map_err(Into::into)
                        .map(|v| (coin_id, v.unwrap().into_owned()))
                })
            })
            .try_collect()
            .unwrap()
    }
}
