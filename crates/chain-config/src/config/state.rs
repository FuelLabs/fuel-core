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
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::DaBlockHeight,
    },
    fuel_types::{
        Address,
        BlockHeight,
        Bytes32,
    },
    fuel_vm::SecretKey,
};
use itertools::Itertools;
use serde::{
    Deserialize,
    Serialize,
};

use crate::CoinConfigGenerator;
#[cfg(feature = "std")]
use crate::SnapshotMetadata;

use super::{
    coin::CoinConfig,
    contract::ContractConfig,
    contract_balance::ContractBalanceConfig,
    contract_state::ContractStateConfig,
    message::MessageConfig,
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

impl StateConfig {
    pub fn extend(&mut self, other: Self) {
        self.coins.extend(other.coins);
        self.messages.extend(other.messages);
        self.contracts.extend(other.contracts);
        self.contract_state.extend(other.contract_state);
        self.contract_balance.extend(other.contract_balance);
    }

    pub fn generate_state_config(db: impl ChainStateDb) -> StorageResult<Self> {
        let coins = db.iter_coin_configs().try_collect()?;
        let messages = db.iter_message_configs().try_collect()?;
        let contracts = db.iter_contract_configs().try_collect()?;
        let contract_state = db.iter_contract_state_configs().try_collect()?;
        let contract_balance = db.iter_contract_balance_configs().try_collect()?;
        let block = db.get_last_block()?;

        Ok(Self {
            coins,
            messages,
            contracts,
            contract_state,
            contract_balance,
            block_height: *block.header().height(),
            da_block_height: block.header().da_height,
        })
    }

    #[cfg(feature = "std")]
    pub fn from_snapshot_metadata(
        snapshot_metadata: SnapshotMetadata,
    ) -> anyhow::Result<Self> {
        let reader = crate::StateReader::for_snapshot(snapshot_metadata)?;
        Self::from_reader(&reader)
    }

    pub fn from_reader(reader: &StateReader) -> anyhow::Result<Self> {
        let coins = reader
            .coins()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let messages = reader
            .messages()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let contracts = reader
            .contracts()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let contract_state = reader
            .contract_state()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let contract_balance = reader
            .contract_balance()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let block_height = reader.block_height();
        let da_block_height = reader.da_block_height();

        Ok(Self {
            coins,
            messages,
            contracts,
            contract_state,
            contract_balance,
            block_height,
            da_block_height,
        })
    }

    pub fn from_db(db: impl ChainStateDb) -> anyhow::Result<Self> {
        let coins = db.iter_coin_configs().try_collect()?;
        let messages = db.iter_message_configs().try_collect()?;
        let contracts = db.iter_contract_configs().try_collect()?;
        let contract_state = db.iter_contract_state_configs().try_collect()?;
        let contract_balance = db.iter_contract_balance_configs().try_collect()?;
        let block = db.get_last_block()?;

        Ok(Self {
            coins,
            messages,
            contracts,
            contract_state,
            contract_balance,
            block_height: *block.header().height(),
            da_block_height: block.header().da_height,
        })
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
        ContractConfig,
        Vec<ContractStateConfig>,
        Vec<ContractBalanceConfig>,
    )>;
    /// Returns *all* unspent coin configs available in the database.
    fn iter_coin_configs(&self) -> BoxedIter<StorageResult<CoinConfig>>;
    /// Returns *alive* contract configs available in the database.
    fn iter_contract_configs(&self) -> BoxedIter<StorageResult<ContractConfig>>;

    /// Returns the state of all contracts
    fn iter_contract_state_configs(
        &self,
    ) -> BoxedIter<StorageResult<ContractStateConfig>>;
    /// Returns the balances of all contracts
    fn iter_contract_balance_configs(
        &self,
    ) -> BoxedIter<StorageResult<ContractBalanceConfig>>;
    /// Returns *all* unspent message configs available in the database.
    fn iter_message_configs(&self) -> BoxedIter<StorageResult<MessageConfig>>;
    /// Returns the last available block.
    fn get_last_block(&self) -> StorageResult<CompressedBlock>;
}

pub use reader::{
    IntoIter,
    StateReader,
};
#[cfg(feature = "std")]
pub use writer::StateWriter;
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
    use crate::{
        self as fuel_core_chain_config,
        Group,
    };
    use itertools::Itertools;
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use super::*;

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_coins() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            StateWriter::parquet(&files, writer::ZstdCompressionLevel::Uncompressed)
                .unwrap();
        encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();

        // when
        let coin_groups =
            group_generator.for_each_group(|group| encoder.write_coins(group));
        encoder.close().unwrap();

        let decoded_coin_groups = StateReader::parquet(files)
            .unwrap()
            .coins()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&coin_groups, decoded_coin_groups, skip_n_groups);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_messages() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            StateWriter::parquet(&files, writer::ZstdCompressionLevel::Level1).unwrap();
        encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();

        // when
        let message_groups =
            group_generator.for_each_group(|group| encoder.write_messages(group));
        encoder.close().unwrap();
        let messages_decoded = StateReader::parquet(files)
            .unwrap()
            .messages()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&message_groups, messages_decoded, skip_n_groups);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_contracts() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            StateWriter::parquet(&files, writer::ZstdCompressionLevel::Level1).unwrap();
        encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();

        // when
        let contract_groups =
            group_generator.for_each_group(|group| encoder.write_contracts(group));
        encoder.close().unwrap();
        let contract_decoded = StateReader::parquet(files)
            .unwrap()
            .contracts()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&contract_groups, contract_decoded, skip_n_groups);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_contract_state() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            StateWriter::parquet(&files, writer::ZstdCompressionLevel::Level1).unwrap();
        encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();

        // when
        let contract_state_groups =
            group_generator.for_each_group(|group| encoder.write_contract_state(group));
        encoder.close().unwrap();
        let decoded_contract_state = StateReader::parquet(files)
            .unwrap()
            .contract_state()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(
            &contract_state_groups,
            decoded_contract_state,
            skip_n_groups,
        );
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_contract_balance() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            StateWriter::parquet(&files, writer::ZstdCompressionLevel::Level1).unwrap();
        encoder.write_block_data(0u32.into(), 0u64.into()).unwrap();

        // when
        let contract_balance_groups =
            group_generator.for_each_group(|group| encoder.write_contract_balance(group));
        encoder.close().unwrap();

        let decoded_contract_balance = StateReader::parquet(files)
            .unwrap()
            .contract_balance()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(
            &contract_balance_groups,
            decoded_contract_balance,
            skip_n_groups,
        );
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_block_height() {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            StateWriter::parquet(&files, writer::ZstdCompressionLevel::Level1).unwrap();
        let block_height = 13u32.into();
        let da_block_height = 14u64.into();

        // when
        encoder
            .write_block_data(block_height, da_block_height)
            .unwrap();
        encoder.close().unwrap();

        // then
        let reader = StateReader::parquet(files).unwrap();
        let block_height_decoded = reader.block_height();
        let da_block_height_decoded = reader.da_block_height();
        assert_eq!(block_height, block_height_decoded);
        assert_eq!(da_block_height, da_block_height_decoded);
    }

    #[test]
    fn roundtrip_json_coins() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = StateWriter::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let coin_groups =
            group_generator.for_each_group(|group| encoder.write_coins(group));
        encoder.close().unwrap();

        let decoded_coins = StateReader::json(file, group_size)
            .unwrap()
            .coins()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&coin_groups, decoded_coins, skip_n_groups);
    }

    #[test]
    fn roundtrip_json_messages() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = StateWriter::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let message_groups =
            group_generator.for_each_group(|group| encoder.write_messages(group));
        encoder.close().unwrap();

        let decoded_messages = StateReader::json(&file, group_size)
            .unwrap()
            .messages()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&message_groups, decoded_messages, skip_n_groups);
    }

    #[test]
    fn roundtrip_json_contracts() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = StateWriter::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let contract_groups =
            group_generator.for_each_group(|group| encoder.write_contracts(group));
        encoder.close().unwrap();

        let decoded_contracts = StateReader::json(&file, group_size)
            .unwrap()
            .contracts()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&contract_groups, decoded_contracts, skip_n_groups);
    }

    #[test]
    fn roundtrip_json_contract_state() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = StateWriter::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let contract_state_groups =
            group_generator.for_each_group(|group| encoder.write_contract_state(group));
        encoder.close().unwrap();

        let decoded_contract_state = StateReader::json(&file, group_size)
            .unwrap()
            .contract_state()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(
            &contract_state_groups,
            decoded_contract_state,
            skip_n_groups,
        );
    }

    #[test]
    fn roundtrip_json_contract_balance() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = StateWriter::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let contract_balance_groups =
            group_generator.for_each_group(|group| encoder.write_contract_balance(group));
        encoder.close().unwrap();

        let decoded_contract_balance = StateReader::json(&file, group_size)
            .unwrap()
            .contract_balance()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(
            &contract_balance_groups,
            decoded_contract_balance,
            skip_n_groups,
        );
    }

    #[test]
    fn roundtrip_json_block_height() {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");
        let block_height = 13u32.into();
        let da_block_height = 14u64.into();
        let mut encoder = StateWriter::json(&file);

        // when
        encoder
            .write_block_data(block_height, da_block_height)
            .unwrap();
        encoder.close().unwrap();

        // then
        let reader = StateReader::json(&file, 100).unwrap();
        let block_height_decoded = reader.block_height();
        let da_block_height_decoded = reader.da_block_height();
        assert_eq!(block_height, block_height_decoded);
        assert_eq!(da_block_height, da_block_height_decoded);
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
        fn for_each_group<T: fuel_core_chain_config::Randomize + Clone>(
            &mut self,
            mut f: impl FnMut(Vec<T>) -> anyhow::Result<()>,
        ) -> Vec<Group<T>> {
            let groups = self.generate_groups();
            for group in &groups {
                f(group.data.clone()).unwrap();
            }
            groups
        }
        fn generate_groups<T: fuel_core_chain_config::Randomize>(
            &mut self,
        ) -> Vec<Group<T>> {
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
}
