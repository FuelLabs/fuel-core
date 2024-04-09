use super::{
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
        Messages,
        Transactions,
    },
    ContractsAssetKey,
    ContractsStateKey,
    Mappable,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::contract::ContractUtxoInfo,
    fuel_types::BlockHeight,
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
#[cfg(feature = "test-helpers")]
use fuel_core_types::{
    fuel_types::Address,
    fuel_types::Bytes32,
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

#[derive(Debug, Clone, Default)]
pub struct StateConfigBuilder {
    coins: Vec<TableEntry<Coins>>,
    messages: Vec<TableEntry<Messages>>,
    contract_state: Vec<TableEntry<ContractsState>>,
    contract_balance: Vec<TableEntry<ContractsAssets>>,
    contract_code: Vec<TableEntry<ContractsRawCode>>,
    contract_utxo: Vec<TableEntry<ContractsLatestUtxo>>,
    block_height: Option<BlockHeight>,
    da_block_height: Option<DaBlockHeight>,
}

impl StateConfigBuilder {
    #[cfg(feature = "std")]
    fn block_height(&self) -> Option<BlockHeight> {
        self.block_height
    }

    #[cfg(feature = "std")]
    fn da_block_height(&self) -> Option<DaBlockHeight> {
        self.da_block_height
    }

    pub fn set_block_height(&mut self, block_height: BlockHeight) {
        self.block_height = Some(block_height);
    }

    pub fn set_da_block_height(&mut self, da_block_height: DaBlockHeight) {
        self.da_block_height = Some(da_block_height);
    }

    pub fn merge(&mut self, builder: Self) -> &mut Self {
        self.coins.extend(builder.coins);
        self.messages.extend(builder.messages);
        self.contract_state.extend(builder.contract_state);
        self.contract_balance.extend(builder.contract_balance);
        self.contract_code.extend(builder.contract_code);
        self.contract_utxo.extend(builder.contract_utxo);

        if let Some(block_height) = builder.block_height {
            self.block_height = Some(block_height);
        }

        if let Some(da_block_height) = builder.da_block_height {
            self.da_block_height = Some(da_block_height);
        }

        self
    }

    #[cfg(feature = "std")]
    pub fn build(self) -> anyhow::Result<StateConfig> {
        use std::collections::HashMap;

        let coins = self.coins.into_iter().map(|coin| coin.into()).collect();
        let messages = self
            .messages
            .into_iter()
            .map(|message| message.into())
            .collect();
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

        let block_height = self
            .block_height
            .ok_or_else(|| anyhow::anyhow!("Block height missing"))?;

        let da_block_height = self
            .da_block_height
            .ok_or_else(|| anyhow::anyhow!("Da block height missing"))?;

        Ok(StateConfig {
            coins,
            messages,
            contracts,
            block_height,
            da_block_height,
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
    fn add(&mut self, _entries: Vec<TableEntry<Transactions>>) {
        // Do not include these for now
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
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        builder.add(coins);

        let messages = reader
            .read::<Messages>()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        builder.add(messages);

        let contract_state = reader
            .read::<ContractsState>()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        builder.add(contract_state);

        let contract_balance = reader
            .read::<ContractsAssets>()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        builder.add(contract_balance);

        let contract_code = reader
            .read::<ContractsRawCode>()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        builder.add(contract_code);

        let contract_utxo = reader
            .read::<ContractsLatestUtxo>()?
            .map_ok(|batch| batch.data)
            .flatten_ok()
            .try_collect()?;

        builder.add(contract_utxo);

        let block_height = reader.block_height();
        builder.set_block_height(block_height);

        let da_block_height = reader.da_block_height();
        builder.set_da_block_height(da_block_height);

        builder.build()
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
    IntoIter,
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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Group<T> {
    pub index: usize,
    pub data: Vec<T>,
}
pub(crate) type GroupResult<T> = anyhow::Result<Group<T>>;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        ChainConfig,
        Randomize,
    };

    use fuel_core_types::fuel_types::ChainId;
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

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn fragment_wont_unset_block_heights(
        writer: impl Fn(&Path) -> SnapshotWriter + Copy,
    ) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let create_writer = || writer(temp_dir.path());
        let original_block_height = BlockHeight::from(10);
        let original_da_block_height = DaBlockHeight(11);
        let block_height_fragment = {
            let mut height_writer = create_writer();
            height_writer
                .write_block_data(original_block_height, original_da_block_height)
                .unwrap();
            height_writer.partial_close().unwrap()
        };
        let no_block_height_present_fragment = create_writer().partial_close().unwrap();
        let chain_config_fragment = {
            let mut chain_config_writer = create_writer();
            chain_config_writer.write_chain_config(&ChainConfig::local_testnet());
            chain_config_writer.partial_close().unwrap()
        };

        // when
        let snapshot = [
            block_height_fragment,
            no_block_height_present_fragment,
            chain_config_fragment,
        ]
        .into_iter()
        .reduce(|a, b| a.merge(b).unwrap())
        .unwrap()
        .finalize()
        .unwrap();

        // then
        let reader = SnapshotReader::open(snapshot).unwrap();
        assert_eq!(reader.block_height(), original_block_height);
        assert_eq!(reader.da_block_height(), original_da_block_height);
    }

    #[test]
    fn json_roundtrip_coins_and_messages() {
        let writer = |temp_dir: &Path| SnapshotWriter::json(temp_dir);
        let reader = |metadata: SnapshotMetadata, group_size: usize| {
            SnapshotReader::open_w_config(metadata, group_size).unwrap()
        };

        assert_roundtrip::<Coins>(writer, reader);
        assert_roundtrip::<Messages>(writer, reader);
    }

    #[test]
    fn json_roundtrip_contract_related_tables() {
        // given
        let mut rng = StdRng::seed_from_u64(0);
        let contracts = std::iter::repeat_with(|| ContractConfig::randomize(&mut rng))
            .take(4)
            .collect_vec();

        let tmp_dir = tempfile::tempdir().unwrap();
        let mut writer = SnapshotWriter::json(tmp_dir.path());

        let state = StateConfig {
            contracts,
            ..Default::default()
        };
        writer.write_chain_config(&ChainConfig::local_testnet());

        // when
        let snapshot = writer.write_state_config(state.clone()).unwrap();

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

        macro_rules! write_in_fragments {
                ($($fragment_ty: ty,)*) => {
                [
                $({
                    let mut writer = create_writer();
                    writer
                        .write(AsTable::<$fragment_ty>::as_table(&state_config))
                        .unwrap();
                    writer.partial_close().unwrap()

                }),*
                ]
            }
            }

        let chain_config = ChainConfig::local_testnet();
        let chain_and_height_fragment = {
            let mut writer = create_writer();
            writer
                .write_block_data(state_config.block_height, state_config.da_block_height)
                .unwrap();

            writer.write_chain_config(&chain_config);
            writer.partial_close().unwrap()
        };
        let fragments = write_in_fragments!(
            Coins,
            Messages,
            ContractsState,
            ContractsAssets,
            ContractsRawCode,
            ContractsLatestUtxo,
        );

        // when
        let snapshot = fragments
            .into_iter()
            .chain([chain_and_height_fragment])
            .reduce(|fragment, next_fragment| fragment.merge(next_fragment).unwrap())
            .unwrap()
            .finalize()
            .unwrap();

        // then
        let reader = SnapshotReader::open(snapshot).unwrap();

        let read_state_config = StateConfig::from_reader(&reader).unwrap();
        assert_eq!(read_state_config, state_config);
        assert_eq!(reader.chain_config(), &chain_config);
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn chain_config_can_be_overridden_with_new_fragments(
        writer: impl Fn(&Path) -> SnapshotWriter + Copy,
    ) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let create_writer = || writer(temp_dir.path());

        let original_chain_config = ChainConfig::local_testnet();
        let original_chain_config_fragment = {
            let mut chain_config_writer = create_writer();
            chain_config_writer.write_chain_config(&original_chain_config);
            chain_config_writer.partial_close().unwrap()
        };
        let chain_config_override = {
            let mut chain_config = ChainConfig::local_testnet();
            chain_config
                .consensus_parameters
                .set_chain_id(ChainId::new(u64::MAX));
            chain_config
        };
        let chain_config_override_fragment = {
            let mut chain_config_writer = create_writer();
            chain_config_writer.write_chain_config(&chain_config_override);
            chain_config_writer.partial_close().unwrap()
        };
        let block_height_fragment = {
            let mut height_writer = create_writer();
            height_writer
                .write_block_data(BlockHeight::from(10), DaBlockHeight(11))
                .unwrap();
            height_writer.partial_close().unwrap()
        };

        // when
        let snapshot = [
            block_height_fragment,
            original_chain_config_fragment,
            chain_config_override_fragment,
        ]
        .into_iter()
        .reduce(|a, b| a.merge(b).unwrap())
        .unwrap()
        .finalize()
        .unwrap();

        // then
        let reader = SnapshotReader::open(snapshot).unwrap();
        assert_eq!(reader.chain_config(), &chain_config_override);
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn empty_fragments_wont_unset_chain_config(
        writer: impl Fn(&Path) -> SnapshotWriter + Copy,
    ) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let create_writer = || writer(temp_dir.path());
        let original_chain_config = ChainConfig::local_testnet();
        let chain_config_fragment = {
            let mut chain_config_writer = create_writer();
            chain_config_writer.write_chain_config(&original_chain_config);
            chain_config_writer.partial_close().unwrap()
        };

        let block_height_fragment = {
            let mut height_writer = create_writer();
            height_writer
                .write_block_data(BlockHeight::from(10), DaBlockHeight(11))
                .unwrap();
            height_writer.partial_close().unwrap()
        };
        let no_chain_config_present_fragment = create_writer().partial_close().unwrap();

        // when
        let snapshot = [
            chain_config_fragment,
            no_chain_config_present_fragment,
            block_height_fragment,
        ]
        .into_iter()
        .reduce(|a, b| a.merge(b).unwrap())
        .unwrap()
        .finalize()
        .unwrap();

        // then
        let read_chain_config = SnapshotReader::open(snapshot)
            .unwrap()
            .chain_config()
            .clone();
        assert_eq!(read_chain_config, original_chain_config);
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn roundtrip_block_heights(writer: impl FnOnce(&Path) -> SnapshotWriter) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let block_height = 13u32.into();
        let da_block_height = 14u64.into();
        let mut writer = writer(temp_dir.path());
        writer.write_chain_config(&ChainConfig::local_testnet());

        // when
        writer
            .write_block_data(block_height, da_block_height)
            .unwrap();
        let snapshot = writer.close().unwrap();

        // then
        let reader = SnapshotReader::open(snapshot).unwrap();

        let block_height_decoded = reader.block_height();
        pretty_assertions::assert_eq!(block_height, block_height_decoded);

        let da_block_height_decoded = reader.da_block_height();
        pretty_assertions::assert_eq!(da_block_height, da_block_height_decoded);
    }

    #[test_case::test_case(given_parquet_writer)]
    #[test_case::test_case(given_json_writer)]
    fn fragment_can_override_block_height(
        writer: impl Fn(&Path) -> SnapshotWriter + Copy,
    ) {
        // given
        let temp_dir = tempfile::tempdir().unwrap();
        let create_writer = || writer(temp_dir.path());
        let original_block_height = BlockHeight::from(10);
        let original_da_block_height = DaBlockHeight(11);
        let block_height_fragment = {
            let mut height_writer = create_writer();
            height_writer
                .write_block_data(original_block_height, original_da_block_height)
                .unwrap();
            height_writer.partial_close().unwrap()
        };

        let block_height_override = BlockHeight::from(20);
        let da_block_height_override = DaBlockHeight(21);
        let block_height_override_fragment = {
            let mut height_writer = create_writer();
            height_writer
                .write_block_data(block_height_override, da_block_height_override)
                .unwrap();
            height_writer.partial_close().unwrap()
        };

        let chain_config_fragment = {
            let mut chain_config_writer = create_writer();
            chain_config_writer.write_chain_config(&ChainConfig::local_testnet());
            chain_config_writer.partial_close().unwrap()
        };

        // when
        let snapshot = [
            chain_config_fragment,
            block_height_fragment,
            block_height_override_fragment,
        ]
        .into_iter()
        .reduce(|a, b| a.merge(b).unwrap())
        .unwrap()
        .finalize()
        .unwrap();

        // then
        let reader = SnapshotReader::open(snapshot).unwrap();
        pretty_assertions::assert_eq!(block_height_override, reader.block_height());
        pretty_assertions::assert_eq!(da_block_height_override, reader.da_block_height());
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
        snapshot_writer.write_chain_config(&ChainConfig::local_testnet());

        // when
        let expected_groups = group_generator
            .write_groups::<T>(&mut snapshot_writer)
            .into_iter()
            .collect_vec();
        snapshot_writer
            .write_block_data(10.into(), DaBlockHeight(11))
            .unwrap();
        let snapshot = snapshot_writer.close().unwrap();

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
            StateConfigBuilder: AddTable<T>,
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
}
