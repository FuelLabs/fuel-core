use bech32::{
    ToBase32,
    Variant::Bech32m,
};
use core::{
    fmt::Debug,
    str::FromStr,
};
use fuel_core_storage::{
    iter::BoxedIter,
    structured_storage::TableWithBlueprint,
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
    },
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::DaBlockHeight,
    },
    entities::{
        coins::coin::CompressedCoin,
        contract::{
            ContractUtxoInfo,
            ContractsInfoType,
            ContractsInfoTypeV1,
        },
    },
    fuel_tx::{
        ContractId,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        Address,
        BlockHeight,
        Bytes32,
    },
    fuel_vm::{
        Salt,
        SecretKey,
    },
};
use itertools::Itertools;
use serde::{
    Deserialize,
    Serialize,
};
use std::collections::HashMap;

use crate::CoinConfigGenerator;
#[cfg(feature = "std")]
use crate::SnapshotMetadata;

use super::{
    coin::CoinConfig,
    contract::ContractConfig,
    contract_balance::ContractBalanceConfig,
    contract_state::ContractStateConfig,
    message::MessageConfig,
    my_entry::MyEntry,
};

#[cfg(feature = "parquet")]
mod parquet;
mod reader;
#[cfg(feature = "std")]
mod writer;

// Fuel Network human-readable part for bech32 encoding
pub const FUEL_BECH32_HRP: &str = "fuel";
pub const TESTNET_INITIAL_BALANCE: u64 = 10_000_000;

pub const TESTNET_WALLET_SECRETS: [&str; 5] = [
    "0xde97d8624a438121b86a1956544bd72ed68cd69f2c99555b08b1e8c51ffd511c",
    "0x37fa81c84ccd547c30c176b118d5cb892bdb113e8e80141f266519422ef9eefd",
    "0x862512a2363db2b3a375c0d4bbbd27172180d89f23f2e259bac850ab02619301",
    "0x976e5c3fa620092c718d852ca703b6da9e3075b9f2ecb8ed42d9f746bf26aafb",
    "0x7f8a325504e7315eda997db7861c9447f5c3eff26333b20180475d94443a10c6",
];

pub const STATE_CONFIG_FILENAME: &str = "state_config.json";

#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct StateConfig {
    /// Spendable coins
    pub coins: Vec<CoinConfig>,
    /// Messages from Layer 1
    pub messages: Vec<MessageConfig>,
    /// Contracts
    pub contracts: Vec<ContractConfig>,
    /// State entries of all contracts
    pub contract_state: Vec<ContractStateConfig>,
    /// Balance entries of all contracts
    pub contract_balance: Vec<ContractBalanceConfig>,
    /// Block height
    pub block_height: BlockHeight,
    /// Da block height
    pub da_block_height: DaBlockHeight,
}

pub trait AsTable<T>
where
    T: TableWithBlueprint,
{
    fn as_table(&self) -> Vec<MyEntry<T>>;
}

impl AsTable<Coins> for StateConfig {
    fn as_table(&self) -> Vec<MyEntry<Coins>> {
        self.coins
            .clone()
            .into_iter()
            .map(|coin| coin.into())
            .collect()
    }
}

impl AsTable<Messages> for StateConfig {
    fn as_table(&self) -> Vec<MyEntry<Messages>> {
        self.messages
            .clone()
            .into_iter()
            .map(|message| message.into())
            .collect()
    }
}

impl AsTable<ContractsState> for StateConfig {
    fn as_table(&self) -> Vec<MyEntry<ContractsState>> {
        self.contract_state
            .clone()
            .into_iter()
            .map(|state| state.into())
            .collect()
    }
}
impl AsTable<ContractsAssets> for StateConfig {
    fn as_table(&self) -> Vec<MyEntry<ContractsAssets>> {
        self.contract_balance
            .clone()
            .into_iter()
            .map(|balance| balance.into())
            .collect()
    }
}

impl AsTable<ContractsRawCode> for StateConfig {
    fn as_table(&self) -> Vec<MyEntry<ContractsRawCode>> {
        self.contracts
            .iter()
            .map(|config| MyEntry {
                key: config.contract_id,
                value: config.code.as_slice().into(),
            })
            .collect()
    }
}
impl AsTable<ContractsInfo> for StateConfig {
    fn as_table(&self) -> Vec<MyEntry<ContractsInfo>> {
        self.contracts
            .iter()
            .map(|config| MyEntry {
                key: config.contract_id,
                value: ContractsInfoType::V1(config.salt.into()),
            })
            .collect()
    }
}
impl AsTable<ContractsLatestUtxo> for StateConfig {
    fn as_table(&self) -> Vec<MyEntry<ContractsLatestUtxo>> {
        self.contracts
            .iter()
            .map(|config| MyEntry {
                key: config.contract_id,
                value: ContractUtxoInfo::V1(
                    fuel_core_types::entities::contract::ContractUtxoInfoV1 {
                        utxo_id: config.utxo_id(),
                        tx_pointer: config.tx_pointer(),
                    },
                ),
            })
            .collect()
    }
}

impl StateConfig {
    pub fn extend(&mut self, other: Self) {
        self.coins.extend(other.coins);
        self.messages.extend(other.messages);
        self.contracts.extend(other.contracts);
        self.contract_state.extend(other.contract_state);
        self.contract_balance.extend(other.contract_balance);
    }

    pub fn from_tables(
        coins: Vec<MyEntry<Coins>>,
        messages: Vec<MyEntry<Messages>>,
        contract_state: Vec<MyEntry<ContractsState>>,
        contract_balance: Vec<MyEntry<ContractsAssets>>,
        contract_code: Vec<MyEntry<ContractsRawCode>>,
        contract_info: Vec<MyEntry<ContractsInfo>>,
        contract_utxo: Vec<MyEntry<ContractsLatestUtxo>>,
        da_block_height: DaBlockHeight,
        block_height: BlockHeight,
    ) -> Self {
        let coins = coins.into_iter().map(|coin| coin.into()).collect();
        let messages = messages.into_iter().map(|message| message.into()).collect();
        let contract_state = contract_state
            .into_iter()
            .map(|state| state.into())
            .collect();

        let contract_balance = contract_balance
            .into_iter()
            .map(|balance| balance.into())
            .collect();

        let contract_code: HashMap<ContractId, Vec<u8>> = contract_code
            .into_iter()
            .map(|entry| (entry.key, entry.value.into()))
            .collect();

        let contract_salt: HashMap<ContractId, Salt> = contract_info
            .into_iter()
            .map(|entry| match entry.value {
                ContractsInfoType::V1(info) => (entry.key, info.into()),
                _ => unreachable!(),
            })
            .collect();

        let contract_utxos: HashMap<ContractId, (UtxoId, TxPointer)> = contract_utxo
            .into_iter()
            .map(|entry| match entry.value {
                ContractUtxoInfo::V1(utxo) => {
                    (entry.key, (utxo.utxo_id, utxo.tx_pointer))
                }
                _ => unreachable!(),
            })
            .collect();

        let all_ids = contract_code
            .keys()
            .chain(contract_salt.keys())
            .chain(contract_utxos.keys())
            .unique()
            .sorted()
            .collect::<Vec<_>>();

        let contracts = all_ids
            .into_iter()
            .filter_map(|id| {
                let code = contract_code.get(id).cloned();
                let salt = contract_salt.get(id).cloned();
                let utxo = contract_utxos.get(id).cloned();
                match (code, salt, utxo) {
                    (Some(code), Some(salt), Some((utxo_id, tx_pointer))) => {
                        Some(ContractConfig {
                            contract_id: *id,
                            code,
                            salt,
                            tx_id: *utxo_id.tx_id(),
                            output_index: utxo_id.output_index(),
                            tx_pointer_block_height: tx_pointer.block_height(),
                            tx_pointer_tx_idx: tx_pointer.tx_index(),
                        })
                    }
                    _ => None,
                }
            })
            .collect();
        Self {
            coins,
            messages,
            contracts,
            contract_state,
            contract_balance,
            block_height,
            da_block_height,
        }
    }

    pub fn generate_state_config(db: impl ChainStateDb) -> StorageResult<Self> {
        let coins = db.iter_coin_configs().try_collect()?;
        let messages = db.iter_message_configs().try_collect()?;
        let contract_state = db.iter_contract_state_configs().try_collect()?;
        let contract_balance = db.iter_contract_balance_configs().try_collect()?;
        let contract_code = db.iter_contracts_code().try_collect()?;
        let contract_info = db.iter_contracts_info().try_collect()?;
        let contract_utxo = db.iter_contracts_latest_utxo().try_collect()?;
        let block = db.get_last_block()?;
        let da_block_height = block.header().da_height;
        let block_height = *block.header().height();

        Ok(Self::from_tables(
            coins,
            messages,
            contract_state,
            contract_balance,
            contract_code,
            contract_info,
            contract_utxo,
            da_block_height,
            block_height,
        ))
    }

    #[cfg(feature = "std")]
    pub fn from_snapshot_metadata(
        snapshot_metadata: SnapshotMetadata,
    ) -> anyhow::Result<Self> {
        let reader = crate::SnapshotReader::for_snapshot(snapshot_metadata)?;
        Self::from_reader(&reader)
    }

    pub fn from_reader(reader: &SnapshotReader) -> anyhow::Result<Self> {
        let coins = reader
            .read()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        let messages = reader
            .read()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        let contract_state = reader
            .read()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        let contract_balance = reader
            .read()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        let contract_code = reader
            .read()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        let contract_info = reader
            .read()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        let contract_utxo = reader
            .read()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        let block_height = reader.block_height();
        let da_block_height = reader.da_block_height();

        Ok(Self::from_tables(
            coins,
            messages,
            contract_state,
            contract_balance,
            contract_code,
            contract_info,
            contract_utxo,
            da_block_height,
            block_height,
        ))
    }

    // TODO: this seems to be a duplicate, remove it
    pub fn from_db(db: impl ChainStateDb) -> anyhow::Result<Self> {
        Ok(Self::generate_state_config(db)?)
    }

    pub fn local_testnet() -> Self {
        // endow some preset accounts with an initial balance
        tracing::info!("Initial Accounts");

        let mut coin_generator = CoinConfigGenerator::new();
        let coins = TESTNET_WALLET_SECRETS
            .into_iter()
            .map(|secret| {
                let secret = SecretKey::from_str(secret).expect("Expected valid secret");
                let address = Address::from(*secret.public_key().hash());
                let bech32_data = Bytes32::new(*address).to_base32();
                let bech32_encoding =
                    bech32::encode(FUEL_BECH32_HRP, bech32_data, Bech32m).unwrap();
                tracing::info!(
                    "PrivateKey({:#x}), Address({:#x} [bech32: {}]), Balance({})",
                    secret,
                    address,
                    bech32_encoding,
                    TESTNET_INITIAL_BALANCE
                );
                coin_generator.generate_with(secret, TESTNET_INITIAL_BALANCE)
            })
            .collect_vec();

        Self {
            coins,
            ..StateConfig::default()
        }
    }

    #[cfg(feature = "random")]
    pub fn random_testnet() -> Self {
        tracing::info!("Initial Accounts");
        let mut rng = rand::thread_rng();
        let mut coin_generator = CoinConfigGenerator::new();
        let coins = (0..5)
            .map(|_| {
                let secret = SecretKey::random(&mut rng);
                let address = Address::from(*secret.public_key().hash());
                let bech32_data = Bytes32::new(*address).to_base32();
                let bech32_encoding =
                    bech32::encode(FUEL_BECH32_HRP, bech32_data, Bech32m).unwrap();
                tracing::info!(
                    "PrivateKey({:#x}), Address({:#x} [bech32: {}]), Balance({})",
                    secret,
                    address,
                    bech32_encoding,
                    TESTNET_INITIAL_BALANCE
                );
                coin_generator.generate_with(secret, TESTNET_INITIAL_BALANCE)
            })
            .collect_vec();

        Self {
            coins,
            ..StateConfig::default()
        }
    }
}

#[impl_tools::autoimpl(for<T: trait> &T, &mut T)]
pub trait ChainStateDb {
    /// Returns the contract config along with its state and balance
    fn get_contract_by_id(
        &self,
        contract_id: fuel_core_types::fuel_types::ContractId,
    ) -> StorageResult<(
        MyEntry<ContractsRawCode>,
        MyEntry<ContractsInfo>,
        MyEntry<ContractsLatestUtxo>,
        Vec<MyEntry<ContractsState>>,
        Vec<MyEntry<ContractsAssets>>,
    )>;
    /// Returns *all* unspent coin configs available in the database.
    fn iter_coin_configs(&self) -> BoxedIter<StorageResult<MyEntry<Coins>>>;

    fn iter_contracts_code(&self) -> BoxedIter<StorageResult<MyEntry<ContractsRawCode>>>;

    fn iter_contracts_info(&self) -> BoxedIter<StorageResult<MyEntry<ContractsInfo>>>;

    fn iter_contracts_latest_utxo(
        &self,
    ) -> BoxedIter<StorageResult<MyEntry<ContractsLatestUtxo>>>;

    /// Returns the state of all contracts
    fn iter_contract_state_configs(
        &self,
    ) -> BoxedIter<StorageResult<MyEntry<ContractsState>>>;
    /// Returns the balances of all contracts
    fn iter_contract_balance_configs(
        &self,
    ) -> BoxedIter<StorageResult<MyEntry<ContractsAssets>>>;
    /// Returns *all* unspent message configs available in the database.
    fn iter_message_configs(&self) -> BoxedIter<StorageResult<MyEntry<Messages>>>;
    /// Returns the last available block.
    fn get_last_block(&self) -> StorageResult<CompressedBlock>;
}

pub use reader::{
    IntoIter,
    SnapshotReader,
};
#[cfg(feature = "std")]
pub use writer::SnapshotWriter;
#[cfg(feature = "parquet")]
pub use writer::ZstdCompressionLevel;
pub const MAX_GROUP_SIZE: usize = usize::MAX;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Group<T> {
    pub index: usize,
    pub data: Vec<T>,
}
pub(crate) type GroupResult<T> = anyhow::Result<Group<T>>;

// #[cfg(all(feature = "std", feature = "random"))]
// #[cfg(test)]
// mod tests {
//     use crate::{
//         self as fuel_core_chain_config,
//         Group,
//     };
//     use itertools::Itertools;
//     use rand::{
//         rngs::StdRng,
//         SeedableRng,
//     };
//
//     use super::*;
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn roundtrip_parquet_coins() {
//         // given
//
//         use std::env::temp_dir;
//         let skip_n_groups = 3;
//         let temp_dir = tempfile::tempdir().unwrap();
//
//         let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
//         let mut encoder = SnapshotWriter::parquet(
//             &temp_dir.path(),
//             writer::ZstdCompressionLevel::Uncompressed,
//         )
//         .unwrap();
//         encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();
//
//         // when
//         let expected_coins = group_generator
//             .for_each_group(|group| encoder.write_coins(group))
//             .into_iter()
//             .map(|group| {
//                 let converted = group
//                     .data
//                     .into_iter()
//                     .map(|coin| MyEntry::<Coins>::from(coin))
//                     .collect();
//                 Group {
//                     index: group.index,
//                     data: converted,
//                 }
//             })
//             .collect_vec();
//         let snapshot = encoder.close().unwrap();
//
//         let decoded_coin_groups = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .coins()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(&expected_coins, decoded_coin_groups, skip_n_groups);
//     }
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn roundtrip_parquet_messages() {
//         // given
//         let skip_n_groups = 3;
//         let temp_dir = tempfile::tempdir().unwrap();
//
//         let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
//         let mut encoder = SnapshotWriter::parquet(
//             &temp_dir.path(),
//             writer::ZstdCompressionLevel::Level1,
//         )
//         .unwrap();
//         encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();
//
//         // when
//         let message_groups =
//             group_generator.for_each_group(|group| encoder.write_messages(group));
//         let snapshot = encoder.close().unwrap();
//         let messages_decoded = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .messages()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(&message_groups, messages_decoded, skip_n_groups);
//     }
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn roundtrip_parquet_contracts() {
//         // given
//         let skip_n_groups = 3;
//         let temp_dir = tempfile::tempdir().unwrap();
//
//         let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
//         let mut encoder = SnapshotWriter::parquet(
//             &temp_dir.path(),
//             writer::ZstdCompressionLevel::Level1,
//         )
//         .unwrap();
//         encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();
//
//         // when
//         let contract_groups =
//             group_generator.for_each_group(|group| encoder.write_contracts(group));
//         let snapshot = encoder.close().unwrap();
//         let contract_decoded = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .contracts()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(&contract_groups, contract_decoded, skip_n_groups);
//     }
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn roundtrip_parquet_contract_state() {
//         // given
//         let skip_n_groups = 3;
//         let temp_dir = tempfile::tempdir().unwrap();
//
//         let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
//         let mut encoder = SnapshotWriter::parquet(
//             &temp_dir.path(),
//             writer::ZstdCompressionLevel::Level1,
//         )
//         .unwrap();
//         encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();
//
//         // when
//         let contract_state_groups =
//             group_generator.for_each_group(|group| encoder.write_contract_state(group));
//         let snapshot = encoder.close().unwrap();
//         let decoded_contract_state = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .contract_state()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(
//             &contract_state_groups,
//             decoded_contract_state,
//             skip_n_groups,
//         );
//     }
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn roundtrip_parquet_contract_balance() {
//         // given
//         let skip_n_groups = 3;
//         let temp_dir = tempfile::tempdir().unwrap();
//
//         let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
//         let mut encoder = SnapshotWriter::parquet(
//             &temp_dir.path(),
//             writer::ZstdCompressionLevel::Level1,
//         )
//         .unwrap();
//         encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();
//
//         // when
//         let contract_balance_groups =
//             group_generator.for_each_group(|group| encoder.write_contract_balance(group));
//         let snapshot = encoder.close().unwrap();
//         let decoded_contract_balance = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .contract_balance()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(
//             &contract_balance_groups,
//             decoded_contract_balance,
//             skip_n_groups,
//         );
//     }
//
//     #[cfg(feature = "parquet")]
//     #[test]
//     fn roundtrip_parquet_block_height() {
//         // given
//         let temp_dir = tempfile::tempdir().unwrap();
//         let mut encoder = SnapshotWriter::parquet(
//             &temp_dir.path(),
//             writer::ZstdCompressionLevel::Level1,
//         )
//         .unwrap();
//         let block_height = 13u32.into();
//         let da_block_height = 14u64.into();
//
//         // when
//         encoder
//             .write_block_data(block_height, da_block_height)
//             .unwrap();
//         let snapshot = encoder.close().unwrap();
//
//         // then
//         let reader = SnapshotReader::for_snapshot(snapshot).unwrap();
//         let block_height_decoded = reader.block_height();
//         let da_block_height_decoded = reader.da_block_height();
//         assert_eq!(block_height, block_height_decoded);
//         assert_eq!(da_block_height, da_block_height_decoded);
//     }
//
//     #[test]
//     fn roundtrip_json_coins() {
//         // given
//         let skip_n_groups = 3;
//         let group_size = 100;
//         let temp_dir = tempfile::tempdir().unwrap();
//         let file = temp_dir.path().join("state_config.json");
//
//         let mut encoder = SnapshotWriter::json(&file);
//         let mut group_generator =
//             GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);
//
//         // when
//         let coin_groups =
//             group_generator.for_each_group(|group| encoder.write_coins(group));
//
//         let snapshot = encoder.close().unwrap();
//         let decoded_coins = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .coins()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(&coin_groups, decoded_coins, skip_n_groups);
//     }
//
//     #[test]
//     fn roundtrip_json_messages() {
//         // given
//         let skip_n_groups = 3;
//         let group_size = 100;
//         let temp_dir = tempfile::tempdir().unwrap();
//         let file = temp_dir.path().join("state_config.json");
//
//         let mut encoder = SnapshotWriter::json(&file);
//         let mut group_generator =
//             GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);
//
//         // when
//         let message_groups =
//             group_generator.for_each_group(|group| encoder.write_messages(group));
//         let snapshot = encoder.close().unwrap();
//
//         let decoded_messages = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .messages()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(&message_groups, decoded_messages, skip_n_groups);
//     }
//
//     #[test]
//     fn roundtrip_json_contracts() {
//         // given
//         let skip_n_groups = 3;
//         let group_size = 100;
//         let temp_dir = tempfile::tempdir().unwrap();
//         let file = temp_dir.path().join("state_config.json");
//
//         let mut encoder = SnapshotWriter::json(&file);
//         let mut group_generator =
//             GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);
//
//         // when
//         let contract_groups =
//             group_generator.for_each_group(|group| encoder.write_contracts(group));
//         let snapshot = encoder.close().unwrap();
//
//         let decoded_contracts = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .contracts()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(&contract_groups, decoded_contracts, skip_n_groups);
//     }
//
//     #[test]
//     fn roundtrip_json_contract_state() {
//         // given
//         let skip_n_groups = 3;
//         let group_size = 100;
//         let temp_dir = tempfile::tempdir().unwrap();
//         let file = temp_dir.path().join("state_config.json");
//
//         let mut encoder = SnapshotWriter::json(&file);
//         let mut group_generator =
//             GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);
//
//         // when
//         let contract_state_groups =
//             group_generator.for_each_group(|group| encoder.write_contract_state(group));
//         let snapshot = encoder.close().unwrap();
//
//         let decoded_contract_state = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .contract_state()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(
//             &contract_state_groups,
//             decoded_contract_state,
//             skip_n_groups,
//         );
//     }
//
//     #[test]
//     fn roundtrip_json_contract_balance() {
//         // given
//         let skip_n_groups = 3;
//         let group_size = 100;
//         let temp_dir = tempfile::tempdir().unwrap();
//         let file = temp_dir.path().join("state_config.json");
//
//         let mut encoder = SnapshotWriter::json(&file);
//         let mut group_generator =
//             GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);
//
//         // when
//         let contract_balance_groups =
//             group_generator.for_each_group(|group| encoder.write_contract_balance(group));
//         let snapshot = encoder.close().unwrap();
//
//         let decoded_contract_balance = SnapshotReader::for_snapshot(snapshot)
//             .unwrap()
//             .contract_balance()
//             .unwrap()
//             .collect_vec();
//
//         // then
//         assert_groups_identical(
//             &contract_balance_groups,
//             decoded_contract_balance,
//             skip_n_groups,
//         );
//     }
//
//     #[test]
//     fn roundtrip_json_block_height() {
//         // given
//         let temp_dir = tempfile::tempdir().unwrap();
//         let file = temp_dir.path().join("state_config.json");
//         let block_height = 13u32.into();
//         let da_block_height = 14u64.into();
//         let mut encoder = SnapshotWriter::json(&file);
//
//         // when
//         encoder
//             .write_block_data(block_height, da_block_height)
//             .unwrap();
//         let snapshot = encoder.close().unwrap();
//
//         // then
//         let reader = SnapshotReader::for_snapshot(snapshot).unwrap();
//         let block_height_decoded = reader.block_height();
//         let da_block_height_decoded = reader.da_block_height();
//         assert_eq!(block_height, block_height_decoded);
//         assert_eq!(da_block_height, da_block_height_decoded);
//     }
//
//     struct GroupGenerator<R> {
//         rand: R,
//         group_size: usize,
//         num_groups: usize,
//     }
//
//     impl<R: ::rand::RngCore> GroupGenerator<R> {
//         fn new(rand: R, group_size: usize, num_groups: usize) -> Self {
//             Self {
//                 rand,
//                 group_size,
//                 num_groups,
//             }
//         }
//         fn for_each_group<T: fuel_core_chain_config::Randomize + Clone>(
//             &mut self,
//             mut f: impl FnMut(Vec<T>) -> anyhow::Result<()>,
//         ) -> Vec<Group<T>> {
//             let groups = self.generate_groups();
//             for group in &groups {
//                 f(group.data.clone()).unwrap();
//             }
//             groups
//         }
//         fn generate_groups<T: fuel_core_chain_config::Randomize>(
//             &mut self,
//         ) -> Vec<Group<T>> {
//             ::std::iter::repeat_with(|| T::randomize(&mut self.rand))
//                 .chunks(self.group_size)
//                 .into_iter()
//                 .map(|chunk| chunk.collect_vec())
//                 .enumerate()
//                 .map(|(index, data)| Group { index, data })
//                 .take(self.num_groups)
//                 .collect()
//         }
//     }
//
//     fn assert_groups_identical<T>(
//         original: &[Group<T>],
//         read: impl IntoIterator<Item = Result<Group<T>, anyhow::Error>>,
//         skip: usize,
//     ) where
//         Vec<T>: PartialEq,
//         T: PartialEq + std::fmt::Debug,
//     {
//         pretty_assertions::assert_eq!(
//             original[skip..],
//             read.into_iter()
//                 .skip(skip)
//                 .collect::<Result<Vec<_>, _>>()
//                 .unwrap()
//         );
//     }
// }
