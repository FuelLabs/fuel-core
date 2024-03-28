use super::workers::GenesisWorkers;
use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::{off_chain::OffChain, on_chain::OnChain},
        genesis_progress::GenesisMetadata,
        Database,
    },
    service::{config::Config, genesis::UncommittedImportResult},
};
use anyhow::anyhow;
use fuel_core_chain_config::{GenesisCommitment, SnapshotReader, TableEntry};
use fuel_core_storage::{
    iter::IteratorOverTable,
    tables::{
        Coins, ConsensusParametersVersions, ContractsLatestUtxo, ContractsRawCode,
        Messages, StateTransitionBytecodeVersions,
    },
    transactional::{Changes, StorageTransaction, WriteTransaction},
    StorageAsMut,
};
use fuel_core_types::{
    self,
    blockchain::{
        block::Block,
        consensus::{Consensus, Genesis},
        header::{
            ApplicationHeader, ConsensusHeader, ConsensusParametersVersion,
            PartialBlockHeader, StateTransitionBytecodeVersion,
        },
        primitives::{DaBlockHeight, Empty},
        SealedBlock,
    },
    entities::{coins::coin::Coin, Message},
    fuel_types::{BlockHeight, Bytes32},
    services::block_importer::ImportResult,
};

use itertools::Itertools;

pub(crate) async fn import_state(
    db: CombinedDatabase,
    snapshot_reader: SnapshotReader,
) -> anyhow::Result<()> {
    let workers = GenesisWorkers::new(db, snapshot_reader);
    if let Err(e) = workers.run_on_chain_imports().await {
        workers.shutdown();
        workers.finished().await;

        return Err(e);
    }

    Ok(())
}

#[cfg(feature = "test-helpers")]
pub async fn execute_and_commit_genesis_block(
    config: &Config,
    db: &CombinedDatabase,
) -> anyhow::Result<()> {
    let result = import_state(db.clone(), config.snapshot_reader.clone()).await?;
    let importer = fuel_core_importer::Importer::new(
        config.block_importer.clone(),
        db.on_chain().clone(),
        (),
        (),
    );
    importer.commit_result(result).await?;
    Ok(())
}

pub(crate) fn init_coin(
    transaction: &mut StorageTransaction<&mut Database>,
    coin: &TableEntry<Coins>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let utxo_id = coin.key;

    let compressed_coin = Coin {
        utxo_id,
        owner: *coin.value.owner(),
        amount: *coin.value.amount(),
        asset_id: *coin.value.asset_id(),
        tx_pointer: *coin.value.tx_pointer(),
    }
    .compress();

    // ensure coin can't point to blocks in the future
    let coin_height = coin.value.tx_pointer().block_height();
    if coin_height > height {
        return Err(anyhow!(
            "coin tx_pointer height ({coin_height}) cannot be greater than genesis block ({height})"
        ));
    }

    if transaction
        .storage::<Coins>()
        .insert(&utxo_id, &compressed_coin)?
        .is_some()
    {
        return Err(anyhow!("Coin should not exist"));
    }

    Ok(())
}

pub(crate) fn init_contract_latest_utxo(
    transaction: &mut StorageTransaction<&mut Database>,
    entry: &TableEntry<ContractsLatestUtxo>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let contract_id = entry.key;

    if entry.value.tx_pointer().block_height() > height {
        return Err(anyhow!(
            "contract tx_pointer cannot be greater than genesis block"
        ));
    }

    if transaction
        .storage::<ContractsLatestUtxo>()
        .insert(&contract_id, &entry.value)?
        .is_some()
    {
        return Err(anyhow!("Contract utxo should not exist"));
    }

    Ok(())
}

pub(crate) fn init_contract_raw_code(
    transaction: &mut StorageTransaction<&mut Database>,
    entry: &TableEntry<ContractsRawCode>,
) -> anyhow::Result<()> {
    let contract = entry.value.as_ref();
    let contract_id = entry.key;

    // insert contract code
    if transaction
        .storage::<ContractsRawCode>()
        .insert(&contract_id, contract)?
        .is_some()
    {
        return Err(anyhow!("Contract code should not exist"));
    }

    Ok(())
}

pub(crate) fn init_da_message(
    transaction: &mut StorageTransaction<&mut Database>,
    msg: TableEntry<Messages>,
    da_height: DaBlockHeight,
) -> anyhow::Result<()> {
    let message: Message = msg.value;

    if message.da_height() > da_height {
        return Err(anyhow!(
            "message da_height cannot be greater than genesis da block height"
        ));
    }

    if transaction
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
        service::{config::Config, FuelService, Task},
    };
    use fuel_core_chain_config::{
        CoinConfig, ContractConfig, MessageConfig, Randomize, SnapshotReader, StateConfig,
    };
    use fuel_core_services::RunnableService;
    use fuel_core_storage::{
        tables::{Coins, ContractsAssets, ContractsState},
        StorageAsRef,
    };
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        entities::coins::coin::Coin,
        fuel_tx::UtxoId,
        fuel_types::{Address, AssetId, BlockHeight},
    };
    use itertools::Itertools;
    use rand::{rngs::StdRng, Rng, RngCore, SeedableRng};
    use std::vec;

    #[tokio::test]
    async fn config_initializes_block_height() {
        let block_height = BlockHeight::from(99u32);
        let snapshot_reader =
            SnapshotReader::local_testnet().with_state_config(StateConfig {
                block_height,
                ..Default::default()
            });
        let service_config = Config {
            snapshot_reader,
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        assert_eq!(
            block_height,
            db.latest_height()
                .unwrap()
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

        let contracts = std::iter::repeat_with(|| given_contract_config(&mut rng))
            .take(1000)
            .collect_vec();

        let state = StateConfig {
            coins,
            messages,
            contracts,
            block_height: BlockHeight::from(0u32),
            da_block_height: Default::default(),
        };
        let snapshot_reader = SnapshotReader::local_testnet().with_state_config(state);

        let service_config = Config {
            snapshot_reader,
            ..Config::local_node()
        };

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
            block_height: starting_height,
            ..Default::default()
        };
        let snapshot_reader = SnapshotReader::local_testnet().with_state_config(state);

        let service_config = Config {
            snapshot_reader,
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
        // given
        let mut rng = StdRng::seed_from_u64(10);

        let contract = given_contract_config(&mut rng);
        let contract_id = contract.contract_id;
        let states = contract.states.clone();
        let snapshot_reader =
            SnapshotReader::local_testnet().with_state_config(StateConfig {
                contracts: vec![contract],
                ..Default::default()
            });

        let service_config = Config {
            snapshot_reader,
            ..Config::local_node()
        };
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
        let snapshot_reader = SnapshotReader::local_testnet().with_state_config(state);

        let config = Config {
            snapshot_reader,
            ..Config::local_node()
        };

        let db = CombinedDatabase::default();

        super::execute_and_commit_genesis_block(&config, &db)
            .await
            .unwrap();

        let expected_msg: Message = msg.into();

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
        let snapshot_reader =
            SnapshotReader::local_testnet().with_state_config(StateConfig {
                contracts: vec![contract],
                ..Default::default()
            });

        let service_config = Config {
            snapshot_reader,
            ..Config::local_node()
        };

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
            block_height: BlockHeight::from(10u32),
            ..Default::default()
        };
        let snapshot_reader = SnapshotReader::local_testnet().with_state_config(state);

        let service_config = Config {
            snapshot_reader,
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

        let state = StateConfig {
            contracts: vec![ContractConfig {
                // set txpointer height > genesis height
                tx_pointer_block_height: BlockHeight::from(11u32),
                ..given_contract_config(&mut rng)
            }],
            block_height: BlockHeight::from(10u32),
            ..Default::default()
        };
        let snapshot_reader = SnapshotReader::local_testnet().with_state_config(state);

        let service_config = Config {
            snapshot_reader,
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
        let snapshot_reader =
            SnapshotReader::local_testnet().with_state_config(initial_state.clone());

        let service_config = Config {
            snapshot_reader,
            ..Config::local_node()
        };

        let db = CombinedDatabase::default();
        FuelService::from_combined_database(db.clone(), service_config)
            .await
            .unwrap();

        let stored_state = db.read_state_config().unwrap();
        assert_eq!(initial_state, stored_state);
    }
}
