// use std::{
// fs::File,
// io::ErrorKind,
// path::{
// Path,
// PathBuf,
// }, marker::PhantomData,
// };
//
// use json::{
// ChainState,
// JsonBatchReader,
// };
// use parquet::ParquetBatchReader;
//
// use crate::{
// CoinConfig,
// StateConfig, MessageConfig, ContractConfig,
// };
//
// use super::{codec::{
// json,
// parquet,
// BatchReader,
// }, contract_state::ContractState, contract_balance::ContractBalance};
//
//
// pub enum ConfigFileFormat {
// JSON,
// Parquet,
// }
//
// pub struct StateConfigImporter {
// config_dir: PathBuf,
// format: ConfigFileFormat,
// }
//
//
// pub struct StateReaders<R> {
// coins: Box<dyn BatchReader::<CoinConfig, BatchType = R>>,
// messages: Box<dyn BatchReader::<MessageConfig, BatchType = R>>,
// contracts: Box<dyn BatchReader::<ContractConfig, BatchType = R>>,
// contracts_state: Box<dyn BatchReader::<ContractState, BatchType = R>>,
// contracts_balance: Box<dyn BatchReader::<ContractBalance, BatchType = R>>,
// } */
//
// impl StateConfigImporter {
// pub fn new(config_dir: impl AsRef<Path>, format: ConfigFileFormat) -> Self {
// Self {
// config_dir: config_dir.as_ref().to_path_buf(),
// format,
// }
// }
//
// pub fn coins<I>(self) -> anyhow::Result<Box<dyn BatchReader::<CoinConfig, >>> {
// match self.format {
// ConfigFileFormat::JSON => {
// todo!()
// },
// ConfigFileFormat::Parquet => {
// let coin_file = File::open(self.config_dir.join("coins.json"))?;
// let coin_reader = ParquetBatchReader::<_, CoinConfig>::new(coin_file)?;
// let tt = Box::new(coin_reader) as Box<dyn BatchReader::<_, _>>;
// },
// }
//
// todo!();
// }
//
// fn json_readers(self) -> anyhow::Result<()> {
// let path = self.config_dir.join("chain_state.json");
// let contents = std::fs::read(&path)?;
// let chain_state: StateConfig =
// serde_json::from_slice(&contents).map_err(|e| {
// std::io::Error::new(
// ErrorKind::InvalidData,
// anyhow::Error::new(e).context(format!(
// "an error occurred while loading the chain config file {}",
// path.to_string_lossy()
// )),
// )
// })?;
//
// TODO: agdali
// let chain_state = ChainState {
// coins: chain_state.coins.unwrap_or_default(),
// messages: chain_state.messages.unwrap_or_default(),
// contracts: chain_state.contracts.unwrap_or_default(),
// contract_state: vec![],
// contract_balance: vec![],
// };
//
// let batch_reader = JsonBatchReader::from_state(chain_state, 1);
//
// todo!()
// }
//
// fn parquet_reader(self) -> anyhow::Result<StateReaders<ParquetBatchReader<File, >>> {
// let coin_file = File::open(self.config_dir.join("coins.json"))?;
// let coin_reader = ParquetBatchReader::<_, CoinConfig>::new(coin_file)?;
// Box::new(coin_reader) as Box<dyn BatchReader::<CoinConfig, ParquetBatchReader<File, CoinConfig>>>;
//
// let message_file = File::open(self.config_dir.join("messages.json"))?;
// let contract_file = File::open(self.config_dir.join("contracts.json"))?;
// let contract_state_file =
// File::open(self.config_dir.join("contract_state.json"))?;
// let contract_balance_file =
// File::open(self.config_dir.join("contract_balance.json"))?;
//
// let coin_reader = ParquetBatchReader::<_, CoinConfig>::new(coin_file)?;
// let message_reader = ParquetBatchReader::<_, MessageConfig>::new(message_file)?;
// let contract_reader = ParquetBatchReader::<_, ContractConfig>::new(contract_file)?;
// let contract_state_reader =
// ParquetBatchReader::<_, ContractState>::new(contract_state_file)?;
// let contract_balance_reader =
// ParquetBatchReader::<_, ContractBalance>::new(contract_balance_file)?;
//
// let batachr = Box::new(coin_reader) as Box<dyn BatchReader::<CoinConfig, ParquetBatchReader<File, CoinConfig>>>;
// Ok(StateReaders {
// coins: Box::new(coin_reader),
// messages: Box::new(message_reader),
// contracts: Box::new(contract_reader),
// contracts_state: Box::new(contract_state_reader),
// contracts_balance: Box::new(contract_balance_reader),
//
// })
// }
// }
