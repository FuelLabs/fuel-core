use super::{
    blob::BlobConfig,
    coin::CoinConfig,
    contract::ContractConfig,
    message::MessageConfig,
    table_entry::TableEntry,
};
use crate::{
    ContractBalanceConfig,
    ContractStateConfig,
};
use fuel_core_storage::{
    structured_storage::TableWithBlueprint,
    tables::{
        Coins,
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        FuelBlocks,
        Messages,
        ProcessedTransactions,
        SealedBlockConsensus,
        Transactions,
    },
    ContractsAssetKey,
    ContractsStateKey,
    Mappable,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::contract::ContractUtxoInfo,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    fuel_vm::BlobData,
};
use itertools::Itertools;
use serde::{
    Deserialize,
    Serialize,
};

#[cfg(feature = "std")]
use crate::SnapshotMetadata;

#[cfg(feature = "test-helpers")]
use crate::CoinConfigGenerator;
#[cfg(feature = "test-helpers")]
use bech32::{
    ToBase32,
    Variant::Bech32m,
};
#[cfg(feature = "test-helpers")]
use core::str::FromStr;
use fuel_core_storage::tables::merkle::{
    FuelBlockMerkleData,
    FuelBlockMerkleMetadata,
};
use fuel_core_types::blockchain::header::{
    BlockHeader,
    ConsensusParametersVersion,
    StateTransitionBytecodeVersion,
};
#[cfg(feature = "test-helpers")]
use fuel_core_types::{
    fuel_types::Address,
    fuel_vm::SecretKey,
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

#[derive(Default, Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct LastBlockConfig {
    /// The block height of the last block.
    pub block_height: BlockHeight,
    /// The da height used in the last block.
    pub da_block_height: DaBlockHeight,
    /// The version of consensus parameters used to produce last block.
    pub consensus_parameters_version: ConsensusParametersVersion,
    /// The version of state transition function used to produce last block.
    pub state_transition_version: StateTransitionBytecodeVersion,
    /// The Merkle root of all blocks before regenesis.
    pub blocks_root: Bytes32,
}

impl LastBlockConfig {
    pub fn from_header(header: &BlockHeader, blocks_root: Bytes32) -> Self {
        Self {
            block_height: *header.height(),
            da_block_height: header.application().da_height,
            consensus_parameters_version: header
                .application()
                .consensus_parameters_version,
            state_transition_version: header
                .application()
                .state_transition_bytecode_version,
            blocks_root,
        }
    }
}

#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct StateConfig {
    /// Spendable coins
    pub coins: Vec<CoinConfig>,
    /// Messages from Layer 1
    pub messages: Vec<MessageConfig>,
    /// Blobs
    #[serde(default)]
    pub blobs: Vec<BlobConfig>,
    /// Contracts
    pub contracts: Vec<ContractConfig>,
    /// Last block config.
    pub last_block: Option<LastBlockConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct StateConfigBuilder {
    coins: Vec<TableEntry<Coins>>,
    messages: Vec<TableEntry<Messages>>,
    blobs: Vec<TableEntry<BlobData>>,
    contract_state: Vec<TableEntry<ContractsState>>,
    contract_balance: Vec<TableEntry<ContractsAssets>>,
    contract_code: Vec<TableEntry<ContractsRawCode>>,
    contract_utxo: Vec<TableEntry<ContractsLatestUtxo>>,
}

impl StateConfigBuilder {
    pub fn merge(&mut self, builder: Self) -> &mut Self {
        self.coins.extend(builder.coins);
        self.messages.extend(builder.messages);
        self.blobs.extend(builder.blobs);
        self.contract_state.extend(builder.contract_state);
        self.contract_balance.extend(builder.contract_balance);
        self.contract_code.extend(builder.contract_code);
        self.contract_utxo.extend(builder.contract_utxo);

        self
    }

    #[cfg(feature = "std")]
    pub fn build(
        self,
        latest_block_config: Option<LastBlockConfig>,
    ) -> anyhow::Result<StateConfig> {
        use std::collections::HashMap;

        let coins = self.coins.into_iter().map(|coin| coin.into()).collect();
        let messages = self
            .messages
            .into_iter()
            .map(|message| message.into())
            .collect();
        let blobs = self.blobs.into_iter().map(|blob| blob.into()).collect();
        let contract_ids = self
            .contract_code
            .iter()
            .map(|entry| entry.key)
            .collect::<Vec<_>>();
        let mut state: HashMap<_, _> = self
            .contract_state
            .into_iter()
            .map(|state| {
                (
                    *state.key.contract_id(),
                    ContractStateConfig {
                        key: *state.key.state_key(),
                        value: state.value.into(),
                    },
                )
            })
            .into_group_map();

        let mut balance: HashMap<_, _> = self
            .contract_balance
            .into_iter()
            .map(|balance| {
                (
                    *balance.key.contract_id(),
                    ContractBalanceConfig {
                        asset_id: *balance.key.asset_id(),
                        amount: balance.value,
                    },
                )
            })
            .into_group_map();

        let mut contract_code: HashMap<_, Vec<u8>> = self
            .contract_code
            .into_iter()
            .map(|entry| (entry.key, entry.value.into()))
            .collect();

        let mut contract_utxos: HashMap<_, _> = self
            .contract_utxo
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
                let (utxo_id, tx_pointer) = contract_utxos
                    .remove(&id)
                    .ok_or_else(|| anyhow::anyhow!("Missing utxo for contract: {id}"))?;
                let states = state.remove(&id).unwrap_or_default();
                let balances = balance.remove(&id).unwrap_or_default();

                Ok(ContractConfig {
                    contract_id: id,
                    code,
                    tx_id: *utxo_id.tx_id(),
                    output_index: utxo_id.output_index(),
                    tx_pointer_block_height: tx_pointer.block_height(),
                    tx_pointer_tx_idx: tx_pointer.tx_index(),
                    states,
                    balances,
                })
            })
            .try_collect()?;

        Ok(StateConfig {
            coins,
            messages,
            blobs,
            contracts,
            last_block: latest_block_config,
        })
    }
}

pub trait AddTable<T>
where
    T: Mappable,
{
    fn add(&mut self, _entries: Vec<TableEntry<T>>);
}

impl AddTable<Coins> for StateConfigBuilder {
    fn add(&mut self, entries: Vec<TableEntry<Coins>>) {
        self.coins.extend(entries);
    }
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

#[cfg(feature = "test-helpers")]
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
            blobs: rand_collection(&mut rng, amount),
            contracts: rand_collection(&mut rng, amount),
            last_block: Some(LastBlockConfig {
                block_height: rng.gen(),
                da_block_height: rng.gen(),
                consensus_parameters_version: rng.gen(),
                state_transition_version: rng.gen(),
                blocks_root: rng.gen(),
            }),
        }
    }
}

pub trait AsTable<T>
where
    T: TableWithBlueprint,
{
    fn as_table(&self) -> Vec<TableEntry<T>>;
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

impl AddTable<Messages> for StateConfigBuilder {
    fn add(&mut self, entries: Vec<TableEntry<Messages>>) {
        self.messages.extend(entries);
    }
}

impl AsTable<BlobData> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<BlobData>> {
        self.blobs
            .clone()
            .into_iter()
            .map(|blob| blob.into())
            .collect()
    }
}

impl AddTable<BlobData> for StateConfigBuilder {
    fn add(&mut self, entries: Vec<TableEntry<BlobData>>) {
        self.blobs.extend(entries);
    }
}

impl AsTable<ContractsState> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ContractsState>> {
        self.contracts
            .iter()
            .flat_map(|contract| {
                contract.states.iter().map(
                    |ContractStateConfig {
                         key: state_key,
                         value: state_value,
                     }| TableEntry {
                        key: ContractsStateKey::new(&contract.contract_id, state_key),
                        value: state_value.clone().into(),
                    },
                )
            })
            .collect()
    }
}

impl AddTable<ContractsState> for StateConfigBuilder {
    fn add(&mut self, entries: Vec<TableEntry<ContractsState>>) {
        self.contract_state.extend(entries);
    }
}

impl AsTable<ContractsAssets> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ContractsAssets>> {
        self.contracts
            .iter()
            .flat_map(|contract| {
                contract.balances.iter().map(
                    |ContractBalanceConfig { asset_id, amount }| TableEntry {
                        key: ContractsAssetKey::new(&contract.contract_id, asset_id),
                        value: *amount,
                    },
                )
            })
            .collect()
    }
}

impl AddTable<ContractsAssets> for StateConfigBuilder {
    fn add(&mut self, entries: Vec<TableEntry<ContractsAssets>>) {
        self.contract_balance.extend(entries);
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

impl AddTable<ContractsRawCode> for StateConfigBuilder {
    fn add(&mut self, entries: Vec<TableEntry<ContractsRawCode>>) {
        self.contract_code.extend(entries);
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

impl AddTable<ContractsLatestUtxo> for StateConfigBuilder {
    fn add(&mut self, entries: Vec<TableEntry<ContractsLatestUtxo>>) {
        self.contract_utxo.extend(entries);
    }
}

impl AsTable<Transactions> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<Transactions>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<Transactions> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<Transactions>>) {}
}

impl AsTable<FuelBlocks> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<FuelBlocks>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<FuelBlocks> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<FuelBlocks>>) {}
}

impl AsTable<SealedBlockConsensus> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<SealedBlockConsensus>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<SealedBlockConsensus> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<SealedBlockConsensus>>) {}
}

impl AsTable<FuelBlockMerkleData> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<FuelBlockMerkleData>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<FuelBlockMerkleData> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<FuelBlockMerkleData>>) {}
}

impl AsTable<FuelBlockMerkleMetadata> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<FuelBlockMerkleMetadata>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<FuelBlockMerkleMetadata> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<FuelBlockMerkleMetadata>>) {}
}

impl AddTable<ProcessedTransactions> for StateConfigBuilder {
    fn add(&mut self, _: Vec<TableEntry<ProcessedTransactions>>) {}
}

impl AsTable<ProcessedTransactions> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ProcessedTransactions>> {
        Vec::new() // Do not include these for now
    }
}

impl StateConfig {
    pub fn sorted(mut self) -> Self {
        self.coins = self
            .coins
            .into_iter()
            .sorted_by_key(|c| c.utxo_id())
            .collect();

        self.messages = self
            .messages
            .into_iter()
            .sorted_by_key(|m| m.nonce)
            .collect();

        self.blobs = self
            .blobs
            .into_iter()
            .sorted_by_key(|b| b.blob_id)
            .collect();

        self.contracts = self
            .contracts
            .into_iter()
            .sorted_by_key(|c| c.contract_id)
            .collect();

        self
    }

    pub fn extend(&mut self, other: Self) {
        self.coins.extend(other.coins);
        self.messages.extend(other.messages);
        self.contracts.extend(other.contracts);
    }

    #[cfg(feature = "std")]
    pub fn from_snapshot_metadata(
        snapshot_metadata: SnapshotMetadata,
    ) -> anyhow::Result<Self> {
        let reader = crate::SnapshotReader::open(snapshot_metadata)?;
        Self::from_reader(&reader)
    }

    #[cfg(feature = "std")]
    pub fn from_reader(reader: &SnapshotReader) -> anyhow::Result<Self> {
        let mut builder = StateConfigBuilder::default();

        let coins = reader
            .read::<Coins>()?
            .into_iter()
            .flatten_ok()
            .try_collect()?;

        builder.add(coins);

        let messages = reader
            .read::<Messages>()?
            .into_iter()
            .flatten_ok()
            .try_collect()?;

        builder.add(messages);

        let blobs = reader
            .read::<BlobData>()?
            .into_iter()
            .flatten_ok()
            .try_collect()?;

        builder.add(blobs);

        let contract_state = reader
            .read::<ContractsState>()?
            .into_iter()
            .flatten_ok()
            .try_collect()?;

        builder.add(contract_state);

        let contract_balance = reader
            .read::<ContractsAssets>()?
            .into_iter()
            .flatten_ok()
            .try_collect()?;

        builder.add(contract_balance);

        let contract_code = reader
            .read::<ContractsRawCode>()?
            .into_iter()
            .flatten_ok()
            .try_collect()?;

        builder.add(contract_code);

        let contract_utxo = reader
            .read::<ContractsLatestUtxo>()?
            .into_iter()
            .flatten_ok()
            .try_collect()?;

        builder.add(contract_utxo);

        builder.build(reader.last_block_config().cloned())
    }

    #[cfg(feature = "test-helpers")]
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

    #[cfg(feature = "test-helpers")]
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

pub use reader::{
    GroupIter,
    Groups,
    SnapshotReader,
};
#[cfg(feature = "parquet")]
pub use writer::ZstdCompressionLevel;
#[cfg(feature = "std")]
pub use writer::{
    SnapshotFragment,
    SnapshotWriter,
};
pub const MAX_GROUP_SIZE: usize = usize::MAX;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        ChainConfig,
        Randomize,
    };

    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use super::*;

    #[test]
    fn parquet_roundtrip() {
        let writer = given_parquet_writer;

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
            BlobData,
            ContractsAssets,
            ContractsLatestUtxo,
            ContractsRawCode,
            ContractsState,
            Messages
        );
    }

    fn given_parquet_writer(path: &Path) -> SnapshotWriter {
        SnapshotWriter::parquet(path, writer::ZstdCompressionLevel::Level1).unwrap()
    }

    fn given_json_writer(path: &Path) -> SnapshotWriter {
        SnapshotWriter::json(path)
    }

    #[test]
    fn json_roundtrip_non_contract_related_tables() {
        let writer = |temp_dir: &Path| SnapshotWriter::json(temp_dir);
        let reader = |metadata: SnapshotMetadata, group_size: usize| {
            SnapshotReader::open_w_config(metadata, group_size).unwrap()
        };

        assert_roundtrip::<Coins>(writer, reader);
        assert_roundtrip::<Messages>(writer, reader);
        assert_roundtrip::<BlobData>(writer, reader);
    }

    #[test]
    fn json_roundtrip_contract_related_tables() {
        // given
        let mut rng = StdRng::seed_from_u64(0);
        let contracts = std::iter::repeat_with(|| ContractConfig::randomize(&mut rng))
            .take(4)
            .collect_vec();

        let tmp_dir = tempfile::tempdir().unwrap();
        let writer = SnapshotWriter::json(tmp_dir.path());

        let state = StateConfig {
            contracts,
            ..Default::default()
        };

        // when
        let snapshot = writer
            .write_state_config(state.clone(), &ChainConfig::local_testnet())
            .unwrap();

        // then
        let reader = SnapshotReader::open(snapshot).unwrap();
        let read_state = StateConfig::from_reader(&reader).unwrap();

        pretty_assertions::assert_eq!(state, read_state);
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn writes_in_fragments_correctly(writer: impl Fn(&Path) -> SnapshotWriter + Copy) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let create_writer = || writer(temp_dir.path());

        let mut rng = StdRng::seed_from_u64(0);
        let state_config = StateConfig::randomize(&mut rng);

        let chain_config = ChainConfig::local_testnet();

        macro_rules! write_in_fragments {
            ($($fragment_ty: ty,)*) => {[
                $({
                    let mut writer = create_writer();
                    writer
                        .write(AsTable::<$fragment_ty>::as_table(&state_config))
                        .unwrap();
                    writer.partial_close().unwrap()
                }),*
            ]}
        }

        let fragments = write_in_fragments!(
            Coins,
            Messages,
            BlobData,
            ContractsState,
            ContractsAssets,
            ContractsRawCode,
            ContractsLatestUtxo,
        );

        // when
        let snapshot = fragments
            .into_iter()
            .reduce(|fragment, next_fragment| fragment.merge(next_fragment).unwrap())
            .unwrap()
            .finalize(state_config.last_block, &chain_config)
            .unwrap();

        // then
        let reader = SnapshotReader::open(snapshot).unwrap();

        let read_state_config = StateConfig::from_reader(&reader).unwrap();
        assert_eq!(read_state_config, state_config);
        assert_eq!(reader.chain_config(), &chain_config);
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn roundtrip_block_heights(writer: impl FnOnce(&Path) -> SnapshotWriter) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let block_height = 13u32.into();
        let da_block_height = 14u64.into();
        let consensus_parameters_version = 321u32;
        let state_transition_version = 123u32;
        let blocks_root = Bytes32::from([123; 32]);
        let block_config = LastBlockConfig {
            block_height,
            da_block_height,
            consensus_parameters_version,
            state_transition_version,
            blocks_root,
        };
        let writer = writer(temp_dir.path());

        // when
        let snapshot = writer
            .close(Some(block_config), &ChainConfig::local_testnet())
            .unwrap();

        // then
        let reader = SnapshotReader::open(snapshot).unwrap();

        let block_config_decoded = reader.last_block_config().cloned();
        pretty_assertions::assert_eq!(Some(block_config), block_config_decoded);
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn missing_tables_tolerated(writer: impl FnOnce(&Path) -> SnapshotWriter) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let writer = writer(temp_dir.path());
        let snapshot = writer.close(None, &ChainConfig::local_testnet()).unwrap();

        let reader = SnapshotReader::open(snapshot).unwrap();

        // when
        let coins = reader.read::<Coins>().unwrap();

        // then
        assert_eq!(coins.into_iter().count(), 0);
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
        StateConfigBuilder: AddTable<T>,
    {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let num_groups = 4;
        let group_size = 1;
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, num_groups);
        let mut snapshot_writer = writer(temp_dir.path());

        // when
        let expected_groups = group_generator
            .write_groups::<T>(&mut snapshot_writer)
            .into_iter()
            .collect_vec();
        let snapshot = snapshot_writer
            .close(None, &ChainConfig::local_testnet())
            .unwrap();

        let actual_groups = reader(snapshot, group_size)
            .read()
            .unwrap()
            .into_iter()
            .collect_vec();

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
        ) -> Vec<Vec<TableEntry<T>>>
        where
            T: TableWithBlueprint,
            T::OwnedKey: serde::Serialize,
            T::OwnedValue: serde::Serialize,
            TableEntry<T>: Randomize,
            StateConfigBuilder: AddTable<T>,
        {
            let groups = self.generate_groups();
            for group in &groups {
                encoder.write(group.clone()).unwrap();
            }
            groups
        }

        fn generate_groups<T>(&mut self) -> Vec<Vec<T>>
        where
            T: Randomize,
        {
            ::std::iter::repeat_with(|| T::randomize(&mut self.rand))
                .chunks(self.group_size)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .take(self.num_groups)
                .collect()
        }
    }

    fn assert_groups_identical<T>(
        original: &[Vec<T>],
        read: impl IntoIterator<Item = Result<Vec<T>, anyhow::Error>>,
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
