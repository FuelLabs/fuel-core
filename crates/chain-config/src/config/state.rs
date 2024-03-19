use bech32::{ToBase32, Variant::Bech32m};
use core::{fmt::Debug, str::FromStr};
use fuel_core_storage::{
    structured_storage::TableWithBlueprint,
    tables::{
        Coins, ContractsAssets, ContractsInfo, ContractsLatestUtxo, ContractsRawCode,
        ContractsState, Messages,
    },
    ContractsAssetKey, ContractsStateKey,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::contract::{ContractUtxoInfo, ContractsInfoType},
    fuel_tx::{ContractId, TxPointer, UtxoId},
    fuel_types::{Address, BlockHeight, Bytes32},
    fuel_vm::{Salt, SecretKey},
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "std")]
use crate::SnapshotMetadata;
use crate::{CoinConfigGenerator, ContractAsset, ContractState};

use super::{
    coin::CoinConfig, contract::ContractConfig, message::MessageConfig,
    table_entry::TableEntry,
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
    /// Block height
    pub block_height: BlockHeight,
    /// Da block height
    pub da_block_height: DaBlockHeight,
}

#[cfg(all(test, feature = "random", feature = "std"))]
impl crate::Randomize for StateConfig {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let amount = 2;
        fn rand_collection<T: crate::Randomize>(
            mut rng: impl rand::Rng,
            amount: usize,
        ) -> Vec<T> {
            std::iter::repeat_with(|| crate::Randomize::randomize(&mut rng))
                .take(amount)
                .collect()
        }

        Self {
            coins: rand_collection(&mut rng, amount),
            messages: rand_collection(&mut rng, amount),
            contracts: rand_collection(&mut rng, amount),
            block_height: rng.gen(),
            da_block_height: rng.gen(),
        }
    }
}

pub trait AsTable<T>
where
    T: TableWithBlueprint,
{
    fn as_table(&self) -> Vec<TableEntry<T>>;
}

impl AsTable<Coins> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<Coins>> {
        self.coins
            .clone()
            .into_iter()
            .map(|coin| coin.into())
            .collect()
    }
}

impl AsTable<Messages> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<Messages>> {
        self.messages
            .clone()
            .into_iter()
            .map(|message| message.into())
            .collect()
    }
}

impl AsTable<ContractsState> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ContractsState>> {
        self.contracts
            .iter()
            .flat_map(|contract| {
                contract.states.iter().map(
                    |ContractState {
                         state_key,
                         state_value,
                     }| TableEntry {
                        key: ContractsStateKey::new(&contract.contract_id, state_key),
                        value: state_value.clone().into(),
                    },
                )
            })
            .collect()
    }
}
impl AsTable<ContractsAssets> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ContractsAssets>> {
        self.contracts
            .iter()
            .flat_map(|contract| {
                contract
                    .balances
                    .iter()
                    .map(|ContractAsset { asset_id, amount }| TableEntry {
                        key: ContractsAssetKey::new(&contract.contract_id, asset_id),
                        value: *amount,
                    })
            })
            .collect()
    }
}

impl AsTable<ContractsRawCode> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ContractsRawCode>> {
        self.contracts
            .iter()
            .map(|config| TableEntry {
                key: config.contract_id,
                value: config.code.as_slice().into(),
            })
            .collect()
    }
}
impl AsTable<ContractsInfo> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ContractsInfo>> {
        self.contracts
            .iter()
            .map(|config| TableEntry {
                key: config.contract_id,
                value: ContractsInfoType::V1(config.salt.into()),
            })
            .collect()
    }
}
impl AsTable<ContractsLatestUtxo> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ContractsLatestUtxo>> {
        self.contracts
            .iter()
            .map(|config| TableEntry {
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
    }

    #[cfg(feature = "std")]
    pub fn from_tables(
        coins: Vec<TableEntry<Coins>>,
        messages: Vec<TableEntry<Messages>>,
        contract_state: Vec<TableEntry<ContractsState>>,
        contract_balance: Vec<TableEntry<ContractsAssets>>,
        contract_code: Vec<TableEntry<ContractsRawCode>>,
        contract_info: Vec<TableEntry<ContractsInfo>>,
        contract_utxo: Vec<TableEntry<ContractsLatestUtxo>>,
        da_block_height: DaBlockHeight,
        block_height: BlockHeight,
    ) -> Self {
        let coins = coins.into_iter().map(|coin| coin.into()).collect();
        let messages = messages.into_iter().map(|message| message.into()).collect();
        let contract_ids = contract_code
            .iter()
            .map(|entry| entry.key)
            .collect::<Vec<_>>();
        let mut state: HashMap<_, _> = contract_state
            .into_iter()
            .map(|state| {
                (
                    *state.key.contract_id(),
                    ContractState {
                        state_key: *state.key.state_key(),
                        state_value: state.value.into(),
                    },
                )
            })
            .into_group_map();

        let mut balance: HashMap<_, _> = contract_balance
            .into_iter()
            .map(|balance| {
                (
                    *balance.key.contract_id(),
                    ContractAsset {
                        asset_id: *balance.key.asset_id(),
                        amount: balance.value,
                    },
                )
            })
            .into_group_map();

        let mut contract_code: HashMap<ContractId, Vec<u8>> = contract_code
            .into_iter()
            .map(|entry| (entry.key, entry.value.into()))
            .collect();

        let mut contract_salt: HashMap<ContractId, Salt> = contract_info
            .into_iter()
            .map(|entry| match entry.value {
                ContractsInfoType::V1(info) => (entry.key, info.into()),
                _ => unreachable!(),
            })
            .collect();

        let mut contract_utxos: HashMap<ContractId, (UtxoId, TxPointer)> = contract_utxo
            .into_iter()
            .map(|entry| match entry.value {
                ContractUtxoInfo::V1(utxo) => {
                    (entry.key, (utxo.utxo_id, utxo.tx_pointer))
                }
                _ => unreachable!(),
            })
            .collect();

        let contracts = contract_ids
            .into_iter()
            .map(|id| -> anyhow::Result<_> {
                let code = contract_code
                    .remove(&id)
                    .ok_or_else(|| anyhow::anyhow!("Missing code for contract: {id}"))?;
                let salt = contract_salt
                    .remove(&id)
                    .ok_or_else(|| anyhow::anyhow!("Missing salt for contract: {id}"))?;
                let (utxo_id, tx_pointer) = contract_utxos
                    .remove(&id)
                    .ok_or_else(|| anyhow::anyhow!("Missing utxo for contract: {id}"))?;
                let states = state.remove(&id).unwrap_or_default();
                let balances = balance.remove(&id).unwrap_or_default();

                Ok(ContractConfig {
                    contract_id: id,
                    code,
                    salt,
                    tx_id: *utxo_id.tx_id(),
                    output_index: utxo_id.output_index(),
                    tx_pointer_block_height: tx_pointer.block_height(),
                    tx_pointer_tx_idx: tx_pointer.tx_index(),
                    states,
                    balances,
                })
            })
            .try_collect()
            .unwrap();
        Self {
            coins,
            messages,
            contracts,
            block_height,
            da_block_height,
        }
    }

    // pub fn generate_state_config(db: impl ChainStateDb) -> StorageResult<Self> {
    //     let coins = db.entries().try_collect()?;
    //     let messages = db.entries().try_collect()?;
    //     let contract_state = db.entries().try_collect()?;
    //     let contract_balance = db.entries().try_collect()?;
    //     let contract_code = db.entries().try_collect()?;
    //     let contract_info = db.entries().try_collect()?;
    //     let contract_utxo = db.entries().try_collect()?;
    //     let block = db.get_last_block()?;
    //     let da_block_height = block.header().da_height;
    //     let block_height = *block.header().height();
    //
    //     Ok(Self::from_tables(
    //         coins,
    //         messages,
    //         contract_state,
    //         contract_balance,
    //         contract_code,
    //         contract_info,
    //         contract_utxo,
    //         da_block_height,
    //         block_height,
    //     ))
    // }

    #[cfg(feature = "std")]
    pub fn from_snapshot_metadata(
        snapshot_metadata: SnapshotMetadata,
    ) -> anyhow::Result<Self> {
        let reader = crate::SnapshotReader::open(snapshot_metadata)?;
        Self::from_reader(&reader)
    }

    #[cfg(feature = "std")]
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

    // // TODO: this seems to be a duplicate, remove it
    // pub fn from_db(db: impl ChainStateDb) -> anyhow::Result<Self> {
    //     Ok(Self::generate_state_config(db)?)
    // }

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

pub use reader::{IntoIter, SnapshotReader};
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

#[cfg(all(feature = "std", feature = "random"))]
#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{Group, Randomize};

    use itertools::Itertools;
    use rand::{rngs::StdRng, SeedableRng};

    use super::*;

    #[cfg(feature = "parquet")]
    mod parquet {
        use std::path::Path;

        use fuel_core_storage::tables::{
            Coins, ContractsAssets, ContractsInfo, ContractsLatestUtxo, ContractsRawCode,
            ContractsState, Messages,
        };

        use crate::{
            config::state::writer, SnapshotMetadata, SnapshotReader, SnapshotWriter,
        };

        use super::{assert_roundtrip, assert_roundtrip_block_heights};

        #[test]
        fn roundtrip() {
            let writer = |temp_dir: &Path| {
                SnapshotWriter::parquet(temp_dir, writer::ZstdCompressionLevel::Level1)
                    .unwrap()
            };

            let reader = |metadata: SnapshotMetadata, _: usize| {
                SnapshotReader::open(metadata).unwrap()
            };

            macro_rules! test_tables {
                ($($table:ty),*) => {
                    $(assert_roundtrip::<$table>(writer, reader);)*
                };
            }

            test_tables!(
                Coins,
                ContractsAssets,
                ContractsInfo,
                ContractsLatestUtxo,
                ContractsRawCode,
                ContractsState,
                Messages
            );
        }

        #[test]
        fn roundtrip_block_heights() {
            let writer = |temp_dir: &Path| {
                SnapshotWriter::parquet(temp_dir, writer::ZstdCompressionLevel::Level1)
                    .unwrap()
            };
            let reader =
                |metadata: SnapshotMetadata| SnapshotReader::open(metadata).unwrap();
            assert_roundtrip_block_heights(writer, reader)
        }
    }

    mod json {
        use std::path::Path;

        use fuel_core_storage::tables::{Coins, Messages};
        use itertools::Itertools;
        use rand::{rngs::StdRng, SeedableRng};

        use crate::{
            ContractConfig, Randomize, SnapshotMetadata, SnapshotReader, SnapshotWriter,
            StateConfig,
        };

        use super::{assert_roundtrip, assert_roundtrip_block_heights};

        #[test]
        fn roundtrip() {
            let writer = |temp_dir: &Path| SnapshotWriter::json(temp_dir);
            let reader = |metadata: SnapshotMetadata, group_size: usize| {
                SnapshotReader::open_w_config(metadata, group_size).unwrap()
            };

            assert_roundtrip::<Coins>(writer, reader);
            assert_roundtrip::<Messages>(writer, reader);
        }

        #[test]
        fn roundtrip_block_heights() {
            let writer = |temp_dir: &Path| SnapshotWriter::json(temp_dir);
            let reader =
                |metadata: SnapshotMetadata| SnapshotReader::open(metadata).unwrap();
            assert_roundtrip_block_heights(writer, reader);
        }

        #[test]
        fn contract_related_tables_roundtrip() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let contracts =
                std::iter::repeat_with(|| ContractConfig::randomize(&mut rng))
                    .take(4)
                    .collect_vec();

            let tmp_dir = tempfile::tempdir().unwrap();
            let writer = SnapshotWriter::json(tmp_dir.path());

            let state = StateConfig {
                contracts,
                ..Default::default()
            };

            // when
            let snapshot = writer.write_state_config(state.clone()).unwrap();

            // then
            let reader = SnapshotReader::open(snapshot).unwrap();
            let read_state = StateConfig::from_reader(&reader).unwrap();

            pretty_assertions::assert_eq!(state, read_state);
        }
    }

    fn assert_roundtrip_block_heights(
        writer: impl FnOnce(&Path) -> SnapshotWriter,
        reader: impl FnOnce(SnapshotMetadata) -> SnapshotReader,
    ) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let block_height = 13u32.into();
        let da_block_height = 14u64.into();
        let mut writer = writer(temp_dir.path());

        // when
        writer
            .write_block_data(block_height, da_block_height)
            .unwrap();
        let snapshot = writer.close().unwrap();

        // then
        let reader = reader(snapshot);

        let block_height_decoded = reader.block_height();
        pretty_assertions::assert_eq!(block_height, block_height_decoded);

        let da_block_height_decoded = reader.da_block_height();
        pretty_assertions::assert_eq!(da_block_height, da_block_height_decoded);
    }

    fn assert_roundtrip<T>(
        writer: impl FnOnce(&Path) -> SnapshotWriter,
        reader: impl FnOnce(SnapshotMetadata, usize) -> SnapshotReader,
    ) where
        T: TableWithBlueprint,
        T::OwnedKey: Randomize
            + serde::Serialize
            + serde::de::DeserializeOwned
            + core::fmt::Debug
            + PartialEq,
        T::OwnedValue: Randomize
            + serde::Serialize
            + serde::de::DeserializeOwned
            + core::fmt::Debug
            + PartialEq,
        StateConfig: AsTable<T>,
        TableEntry<T>: Randomize,
    {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let num_groups = 4;
        let group_size = 1;
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, num_groups);
        let mut encoder = writer(temp_dir.path());

        // when
        let expected_groups = group_generator
            .write_groups::<T>(&mut encoder)
            .into_iter()
            .collect_vec();
        encoder
            .write_block_data(10.into(), DaBlockHeight(11))
            .unwrap();
        let snapshot = encoder.close().unwrap();

        let actual_groups = reader(snapshot, group_size).read().unwrap().collect_vec();

        // then
        assert_groups_identical(&expected_groups, actual_groups, skip_n_groups);
    }

    struct GroupGenerator<R> {
        rand: R,
        group_size: usize,
        num_groups: usize,
    }

    impl<R: ::rand::RngCore> GroupGenerator<R> {
        fn new(rand: R, group_size: usize, num_groups: usize) -> Self {
            Self {
                rand,
                group_size,
                num_groups,
            }
        }

        fn write_groups<T>(
            &mut self,
            encoder: &mut SnapshotWriter,
        ) -> Vec<Group<TableEntry<T>>>
        where
            T: TableWithBlueprint,
            T::OwnedKey: serde::Serialize,
            T::OwnedValue: serde::Serialize,
            TableEntry<T>: Randomize,
        {
            let groups = self.generate_groups();
            for group in &groups {
                encoder.write(group.data.clone()).unwrap();
            }
            groups
        }

        fn generate_groups<T>(&mut self) -> Vec<Group<T>>
        where
            T: Randomize,
        {
            ::std::iter::repeat_with(|| T::randomize(&mut self.rand))
                .chunks(self.group_size)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .enumerate()
                .map(|(index, data)| Group { index, data })
                .take(self.num_groups)
                .collect()
        }
    }

    fn assert_groups_identical<T>(
        original: &[Group<T>],
        read: impl IntoIterator<Item = Result<Group<T>, anyhow::Error>>,
        skip: usize,
    ) where
        Vec<T>: PartialEq,
        T: PartialEq + std::fmt::Debug,
    {
        pretty_assertions::assert_eq!(
            original[skip..],
            read.into_iter()
                .skip(skip)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        );
    }

    mod randomize_impls {
        use fuel_core_types::fuel_tx::UtxoId;

        use fuel_core_types::fuel_types::Bytes32;

        use fuel_core_storage::ContractsStateKey;

        use fuel_core_storage::ContractsAssetKey;

        use fuel_core_storage::ContractsStateData;

        use fuel_core_types::entities::contract::ContractUtxoInfo;

        use fuel_core_types::fuel_vm::Salt;

        use fuel_core_types::entities::contract::ContractsInfoType;

        use fuel_core_types;

        use fuel_core_storage::tables::ContractsLatestUtxo;

        use fuel_core_storage::tables::ContractsInfo;

        use fuel_core_storage::tables::ContractsAssets;

        use fuel_core_storage::tables::ContractsState;

        use fuel_core_types::fuel_tx::ContractId;

        use fuel_core_types::entities::message::MessageV1;

        use fuel_core_types::fuel_types::Nonce;

        use fuel_core_types::entities::message::Message;

        use fuel_core_storage::tables::Messages;

        use fuel_core_storage::tables::ContractsRawCode;

        use fuel_core_storage::tables::Coins;

        use crate::TableEntry;

        use fuel_core_types::entities::coins::coin::CompressedCoinV1;

        use rand;

        use fuel_core_types::entities::coins::coin::CompressedCoin;

        use crate::Randomize;

        macro_rules! delegating_impl {
            ($($table: ty),*) => {
                $(
                    impl Randomize for TableEntry<$table> {
                        fn randomize(mut rng: impl rand::Rng) -> Self {
                            Self {
                                key: Randomize::randomize(&mut rng),
                                value: Randomize::randomize(&mut rng),
                            }
                        }
                    }
                )*
            };
        }

        delegating_impl!(
            Coins,
            ContractsRawCode,
            ContractsState,
            ContractsAssets,
            ContractsInfo,
            ContractsLatestUtxo
        );

        // Messages are special, because they have the key inside of the value as well
        impl Randomize for TableEntry<Messages> {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                let value = Message::randomize(&mut rng);
                Self {
                    key: *value.nonce(),
                    value,
                }
            }
        }

        impl Randomize for CompressedCoin {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                Self::V1(CompressedCoinV1 {
                    owner: rng.gen(),
                    amount: rng.gen(),
                    asset_id: rng.gen(),
                    tx_pointer: rng.gen(),
                })
            }
        }

        impl Randomize for Nonce {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                rng.gen()
            }
        }

        impl Randomize for Message {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                Self::V1(MessageV1 {
                    sender: rng.gen(),
                    recipient: rng.gen(),
                    nonce: rng.gen(),
                    amount: rng.gen(),
                    data: rng.gen::<[u8; 32]>().to_vec(),
                    da_height: rng.gen(),
                })
            }
        }

        impl Randomize for ContractId {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                rng.gen()
            }
        }

        impl Randomize for fuel_core_types::fuel_vm::Contract {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                rng.gen::<[u8; 32]>().to_vec().into()
            }
        }

        impl Randomize for ContractsInfoType {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                let salt: Salt = rng.gen();
                Self::V1(salt.into())
            }
        }

        impl Randomize for ContractUtxoInfo {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                ContractUtxoInfo::V1(
                    fuel_core_types::entities::contract::ContractUtxoInfoV1 {
                        utxo_id: rng.gen(),
                        tx_pointer: rng.gen(),
                    },
                )
            }
        }

        impl Randomize for ContractsStateData {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                rng.gen::<[u8; 32]>().to_vec().into()
            }
        }

        impl Randomize for ContractsAssetKey {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                let contract_id: ContractId = rng.gen();
                let asset_id: fuel_core_types::fuel_types::AssetId = rng.gen();
                Self::new(&contract_id, &asset_id)
            }
        }

        impl Randomize for ContractsStateKey {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                let contract_id: ContractId = rng.gen();
                let state_key: Bytes32 = rng.gen();
                Self::new(&contract_id, &state_key)
            }
        }

        impl Randomize for u64 {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                rng.gen()
            }
        }

        impl Randomize for UtxoId {
            fn randomize(mut rng: impl rand::Rng) -> Self {
                Self::new(rng.gen(), rng.gen())
            }
        }
    }
}
