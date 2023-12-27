use std::path::{
    Path,
    PathBuf,
};

use crate::{
    config::{
        contract_balance::ContractBalance,
        contract_state::ContractStateConfig,
    },
    CoinConfig,
    ContractConfig,
    MessageConfig,
    StateConfig,
};

#[cfg(feature = "parquet")]
use crate::config::codec::parquet::Schema;

#[cfg(feature = "parquet")]
use super::parquet;

enum EncoderType {
    Json {
        buffer: StateConfig,
        state_file_path: PathBuf,
    },
    #[cfg(feature = "parquet")]
    Parquet {
        coins: parquet::Encoder<std::fs::File, CoinConfig>,
        messages: parquet::Encoder<std::fs::File, MessageConfig>,
        contracts: parquet::Encoder<std::fs::File, ContractConfig>,
        contract_state: parquet::Encoder<std::fs::File, ContractStateConfig>,
        contract_balance: parquet::Encoder<std::fs::File, ContractBalance>,
    },
}

pub struct Encoder {
    encoder: EncoderType,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg(feature = "parquet")]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum CompressionLevel {
    Uncompressed,
    Level1,
    Level2,
    Level3,
    Level4,
    Level5,
    Level6,
    Level7,
    Level8,
    Level9,
    Max,
}

#[cfg(feature = "parquet")]
impl TryFrom<u8> for CompressionLevel {
    type Error = anyhow::Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Uncompressed),
            1 => Ok(Self::Level1),
            2 => Ok(Self::Level2),
            3 => Ok(Self::Level3),
            4 => Ok(Self::Level4),
            5 => Ok(Self::Level5),
            6 => Ok(Self::Level6),
            7 => Ok(Self::Level7),
            8 => Ok(Self::Level8),
            9 => Ok(Self::Level9),
            10 => Ok(Self::Max),
            _ => {
                anyhow::bail!("Compression level {value} outside of allowed range 0..=10")
            }
        }
    }
}

#[cfg(feature = "parquet")]
impl From<CompressionLevel> for u8 {
    fn from(value: CompressionLevel) -> Self {
        match value {
            CompressionLevel::Uncompressed => 0,
            CompressionLevel::Level1 => 1,
            CompressionLevel::Level2 => 2,
            CompressionLevel::Level3 => 3,
            CompressionLevel::Level4 => 4,
            CompressionLevel::Level5 => 5,
            CompressionLevel::Level6 => 6,
            CompressionLevel::Level7 => 7,
            CompressionLevel::Level8 => 8,
            CompressionLevel::Level9 => 9,
            CompressionLevel::Max => 10,
        }
    }
}

#[cfg(feature = "parquet")]
impl From<CompressionLevel> for ::parquet::basic::GzipLevel {
    fn from(value: CompressionLevel) -> Self {
        Self::try_new(u8::from(value) as u32)
            .expect("The range 0..=10 is valid for Gzip compression")
    }
}

impl Encoder {
    pub fn json(snapshot_dir: impl AsRef<Path>) -> Self {
        Self {
            encoder: EncoderType::Json {
                state_file_path: snapshot_dir.as_ref().join("chain_state.json"),
                buffer: StateConfig::default(),
            },
        }
    }

    #[cfg(feature = "parquet")]
    pub fn parquet(
        snapshot_dir: impl AsRef<Path>,
        compression_level: CompressionLevel,
    ) -> anyhow::Result<Self> {
        use ::parquet::basic::{
            Compression,
            GzipLevel,
        };
        use std::fs::File;

        fn create_encoder<T>(
            path: &Path,
            name: &str,
            compression: Compression,
        ) -> anyhow::Result<parquet::Encoder<File, T>>
        where
            T: Schema,
        {
            let path = path.join(format!("{name}.parquet"));
            let file = std::fs::File::create(path)?;
            parquet::Encoder::new(file, compression)
        }

        let path = snapshot_dir.as_ref();
        let compression = Compression::GZIP(GzipLevel::from(compression_level));
        Ok(Self {
            encoder: EncoderType::Parquet {
                coins: create_encoder(path, "coins", compression)?,
                messages: create_encoder(path, "messages", compression)?,
                contracts: create_encoder(path, "contracts", compression)?,
                contract_state: create_encoder(path, "contract_state", compression)?,
                contract_balance: create_encoder(path, "contract_balance", compression)?,
            },
        })
    }

    pub fn write_coins(&mut self, elements: Vec<CoinConfig>) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.coins.extend(elements);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet { coins, .. } => coins.write(elements),
        }
    }

    pub fn write_contracts(
        &mut self,
        elements: Vec<ContractConfig>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.contracts.extend(elements);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet { contracts, .. } => contracts.write(elements),
        }
    }

    pub fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.messages.extend(elements);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet { messages, .. } => messages.write(elements),
        }
    }

    pub fn write_contract_state(
        &mut self,
        elements: Vec<ContractStateConfig>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.contract_state.extend(elements);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet { contract_state, .. } => contract_state.write(elements),
        }
    }

    pub fn write_contract_balance(
        &mut self,
        elements: Vec<ContractBalance>,
    ) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderType::Json { buffer: state, .. } => {
                state.contract_balance.extend(elements);
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet {
                contract_balance, ..
            } => contract_balance.write(elements),
        }
    }

    pub fn close(self) -> anyhow::Result<()> {
        match self.encoder {
            EncoderType::Json {
                buffer,
                state_file_path,
            } => {
                let file = std::fs::File::create(state_file_path)?;
                serde_json::to_writer(file, &buffer)?;
                Ok(())
            }
            #[cfg(feature = "parquet")]
            EncoderType::Parquet {
                coins,
                messages,
                contracts,
                contract_state,
                contract_balance,
            } => {
                coins.close()?;
                messages.close()?;
                contracts.close()?;
                contract_state.close()?;
                contract_balance.close()?;
                Ok(())
            }
        }
    }
}

#[cfg(feature = "random")]
#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;

    #[test]
    fn json_encoder_generates_single_file_with_expected_name() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let encoder = Encoder::json(dir.path());

        // when
        encoder.close().unwrap();

        // then
        let entries: Vec<_> = dir.path().read_dir().unwrap().try_collect().unwrap();

        match entries.as_slice() {
            [entry] => assert_eq!(entry.path(), dir.path().join("state.json")),
            _ => panic!("Expected single file \"state.json\""),
        }
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn parquet_encoder_generates_expected_filenames() {
        // given
        let dir = tempfile::tempdir().unwrap();
        let encoder =
            Encoder::parquet(dir.path(), CompressionLevel::Uncompressed).unwrap();

        // when
        encoder.close().unwrap();

        // then
        let entries: std::collections::HashSet<_> = dir
            .path()
            .read_dir()
            .unwrap()
            .map_ok(|entry| entry.path())
            .try_collect()
            .unwrap();
        let expected_files = std::collections::HashSet::from(
            [
                "coins.parquet",
                "messages.parquet",
                "contracts.parquet",
                "contract_state.parquet",
                "contract_balance.parquet",
            ]
            .map(|name| dir.path().join(name)),
        );

        assert_eq!(entries, expected_files);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn parquet_encoder_encodes_types_in_expected_files() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

        test_data_written_in_expected_file::<CoinConfig>(
            &mut rng,
            "coins.parquet",
            |coins, encoder| encoder.write_coins(coins),
        );

        test_data_written_in_expected_file::<MessageConfig>(
            &mut rng,
            "messages.parquet",
            |coins, encoder| encoder.write_messages(coins),
        );

        test_data_written_in_expected_file::<ContractConfig>(
            &mut rng,
            "contracts.parquet",
            |coins, encoder| encoder.write_contracts(coins),
        );

        test_data_written_in_expected_file::<ContractStateConfig>(
            &mut rng,
            "contract_state.parquet",
            |coins, encoder| encoder.write_contract_state(coins),
        );

        test_data_written_in_expected_file::<ContractBalance>(
            &mut rng,
            "contract_balance.parquet",
            |coins, encoder| encoder.write_contract_balance(coins),
        );
    }

    #[cfg(feature = "parquet")]
    fn test_data_written_in_expected_file<T>(
        rng: impl rand::Rng,
        expected_filename: &str,
        write: impl FnOnce(Vec<T>, &mut Encoder) -> anyhow::Result<()>,
    ) where
        parquet::Decoder<std::fs::File, T>:
            Iterator<Item = anyhow::Result<crate::Group<T>>>,
        T: crate::Randomize + PartialEq + ::core::fmt::Debug + Clone,
    {
        use std::fs::File;
        // given
        let dir = tempfile::tempdir().unwrap();
        let mut encoder =
            Encoder::parquet(dir.path(), CompressionLevel::Uncompressed).unwrap();
        let original_data = vec![T::randomize(rng)];

        // when
        write(original_data.clone(), &mut encoder).unwrap();
        encoder.close().unwrap();

        // then
        let file = std::fs::File::open(dir.path().join(expected_filename)).unwrap();
        let decoded = parquet::Decoder::<File, T>::new(file)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(original_data, decoded.first().unwrap().data);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn all_compressions_are_valid() {
        use ::parquet::basic::GzipLevel;
        use strum::IntoEnumIterator;
        for level in CompressionLevel::iter() {
            let _ = GzipLevel::from(level);
        }
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn can_use_all_compression_levels() {
        use std::fs::File;

        use rand::SeedableRng;
        use strum::IntoEnumIterator;

        use crate::Randomize;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        for level in CompressionLevel::iter() {
            // given
            let dir = tempfile::tempdir().unwrap();
            let mut encoder = Encoder::parquet(dir.path(), level).unwrap();
            let coins = vec![CoinConfig::randomize(&mut rng); 10];
            encoder.write_coins(coins.clone()).unwrap();

            let messages = vec![MessageConfig::randomize(&mut rng); 10];
            encoder.write_messages(messages.clone()).unwrap();

            let contracts = vec![ContractConfig::randomize(&mut rng); 10];
            encoder.write_contracts(contracts.clone()).unwrap();

            let contract_state = vec![ContractStateConfig::randomize(&mut rng); 10];
            encoder
                .write_contract_state(contract_state.clone())
                .unwrap();

            let contract_balance = vec![ContractBalance::randomize(&mut rng); 10];
            encoder
                .write_contract_balance(contract_balance.clone())
                .unwrap();

            encoder.close().unwrap();

            let coins_file =
                std::fs::File::open(dir.path().join("coins.parquet")).unwrap();
            let decoded = parquet::Decoder::<File, CoinConfig>::new(coins_file)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            assert_eq!(coins, decoded.first().unwrap().data);

            let messages_file =
                std::fs::File::open(dir.path().join("messages.parquet")).unwrap();
            let decoded = parquet::Decoder::<File, MessageConfig>::new(messages_file)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            assert_eq!(messages, decoded.first().unwrap().data);

            let contracts_file =
                std::fs::File::open(dir.path().join("contracts.parquet")).unwrap();
            let decoded = parquet::Decoder::<File, ContractConfig>::new(contracts_file)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            assert_eq!(contracts, decoded.first().unwrap().data);

            let contract_state_file =
                std::fs::File::open(dir.path().join("contract_state.parquet")).unwrap();
            let decoded =
                parquet::Decoder::<File, ContractStateConfig>::new(contract_state_file)
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();

            assert_eq!(contract_state, decoded.first().unwrap().data);

            let contract_balance_file =
                std::fs::File::open(dir.path().join("contract_balance.parquet")).unwrap();
            let decoded =
                parquet::Decoder::<File, ContractBalance>::new(contract_balance_file)
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();

            assert_eq!(contract_balance, decoded.first().unwrap().data);
        }
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn check_compression_again_using_this_json_data_as_input() {
        use strum::IntoEnumIterator;

        let json_data = r#"
{"coins":[{"tx_id":"0x087f5a4786ee2065fb3d96d09c2b6c6b67f863ea77faccfbff7633f4ddedb04e","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0x094b8ce053d5ace70d0b5f9cd1e0e9a67680092d2a0ac5b2bef14342db51eb50","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0x0bd77bcc811eb73578dbf37057962974e447ea10066fe0ffb753f7628d70b436","output_index":"0x01","tx_pointer_block_height":"0x00000008","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x0000000000015f90","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0x0bd77bcc811eb73578dbf37057962974e447ea10066fe0ffb753f7628d70b436","output_index":"0x02","tx_pointer_block_height":"0x00000008","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0x14075d68c76926b312a2818ca07ba31554d0c8bac1d52802558c042ca8fc84a4","output_index":"0x01","tx_pointer_block_height":"0x00000002","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0x181e247b49a0cffd9d38f1e5933c000f5f0613ce57fc8393bdd8ff9da327517e","output_index":"0x01","tx_pointer_block_height":"0x00000007","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x0000000000015f90","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0x18e27bec04764569ca874fdd1a00e65cfba68143ab027ce33d70369f9032601a","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0x3dd92975673cf7c5d87e94601ea3414bd13e2d5ebb3de20ea187411527852d34","output_index":"0x01","tx_pointer_block_height":"0x00000005","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0x475ff568c9f5586e9dec6ebad1d1850d7dbbd8898dbc29c89ed4cf9b6e205358","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0x4d23e9e7fe23dc1294b98073cede4f08603319b4e25802bb0d2d286af42e1427","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0x6b2c66b877d5dbbb48458ea3a5c2a4d70f9a4e056ae1a6726b92a6d93157467d","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0x890910a3db03061c1aaa1d1923bcfc032d2b6b21686f19dc9c6c0305fba42fca","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0x8c044823910cdbb9bb1902318f18b4fe64765234411db61465bad022c5f31e2c","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0xa9731acba6a6e42908666ee102d172ece34d0e1aa4d6688f062540d2ae3b3ee9","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0xaa8f82d6ec3b135970a32e468dbae098673544a209ff5d6144625c440a9657b4","output_index":"0x01","tx_pointer_block_height":"0x00000006","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x0000000000015f90","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"},{"tx_id":"0xbfbd7209b43ca0e27e285314f54e4f0a4e41838d259c9be88fbef12d3f612a23","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0xc662d98bb2a4b20d6d41a8bde32cdfd4aeeeddf9688a85434f4313ed96e34588","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0xd7c00fc2828e8635fb17dd426bc25092bca59c322306d2a4a23102290abeccff","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0xedfed681c0ef218db7a494271cbefb412cec8b9468929aba9eaa231758db0ea2","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0101010101010101010101010101010101010101010101010101010101010101"},{"tx_id":"0xff66de6924c4d4e202f0cce0e250af24864b2ce4f6f4bca0f19b4591e1584f15","output_index":"0x00","tx_pointer_block_height":"0x00000000","tx_pointer_tx_idx":"0x0000","maturity":"0x00000000","owner":"0x5349930da01b2b94e137c035a499ceff4314e0259f0552ec9e2b5156d8007958","amount":"0x00000000000186a0","asset_id":"0x0000000000000000000000000000000000000000000000000000000000000000"}],"messages":[],"contracts":[{"contract_id":"0x7f9bbb7d0bc2a9698e6dcceb693e9ef682b4a027fbbdf0a440845a01535736a3","code":"0x7400000347000000000000000000072c5dfcc00110fff3005d4060495d47f00d134904407648000b5d47f00e13490440764800775d47f00f13490440764800e35d47f010134904407648013d72f0007b36f000001aec5000910002185d43f011104103005d47f011104513007248002028ed04805fec0004504bb028724c0020284914c05d4fb0045d47f0041b4534405d4bf004104514805d4bf005104514805d4bf0061f4514805d4bf00719493480504fb09872500020284d05005043b1f872500020284135005043b1f8504fb1f85053b0e81ae910001ae5400020f8330058fbe00250fbe004740001721a47d0005053b1081ae810001ae5400020f8330058fbe00250fbe0047400016a1a53d0005057b13872580020285515805047b1b872580020284545805053b178a35154615047b1587250002028453500504fb1785053b198a35114e072440020284144405043b0b872440020284144405047b048724c0020284504c05fec100d5fed200e5043b0485047b1d872480020284504805d43b00d5d47b00e5d4bf0081b4904805d4ff0041b493480264800001a487000504fb1d8394904d0764000065043b0885fec0011504bb12872440010284904407400000a5043b0785fec100f5d4ff0041b453440104524405d4510005fed1010504bb12872440010284904405043b0d872440010284124405d43b0251341004076400001360000005d43b01c244000001aec5000910002185d53f012105143005d43f012104103007244002028ed44405fec00045047b02872480020284504805d4bb0045d43f0041b4124005d47f004104104405d47f005104104405d47f0061f4104405d47f00719452440504bb098724c0020284944c0504fb1f872500020284d2500504bb1f8504fb1f85053b0e81ae900001ae5400020f8330058fbe00250fbe004740001031a43d0005053b1081ae810001ae5400020f8330058fbe00250fbe004740000fb1a53d0005057b13872580020285505805043b1b872580020284145805053b178a35154215043b1587250002028413500504fb1785053b198a35104e072400020284944005043b0b87248002028414480504bb048724c0020284904c05fec100d5fed100e5043b0485047b1d872480020284504805d43b00d5d47b00e5d4bf0081b4904805d4ff0041b493480264800001a487000504fb1d8394904d0764000065043b0885fec0011504bb12872440010284904407400000a5043b0785fec100f5d4ff0041b453440104524405d4510005fed1010504bb12872440010284904405043b0d872440010284124405d43b0251341004076400001360000005d43b01c244000001aec5000910001d85d5c604a5d43f012104103005d47f012104513007248002028ed04805fec0004504bb028724c0020284914c05d47b0045d4bf0041b4914805d4ff004104924c05d4ff005104924c05d4ff0061f4924c05d4ff007194514c0504fb07872500020284d05005043b1b872500020284135005043b1b8504fb1b85053b0b81ae920001ae5400020f8330058fbe00250fbe004740000931a4bd0005053b0d81ae810001ae5400020f8330058fbe00250fbe0047400008b1a53d0005057b0f87258002028552580504bb17872580020284945805053b138a35154a1504bb1187250002028493500504fb1385053b158a35124e072480020284144805043b0987248002028414480504bb048724c0020284904c05fec100d5fed100e5043b0485047b19872480020284504805d43b00d5d47b00e5d4bf0081b4904805d4ff0041b493480264800001a487000504fb198394934d05d4ff0041b453440104524405f4570005047b1983b450490240000001aec5000910001d85d5c604a5d43f011104103005d47f011104513007248002028ed04805fec0004504bb028724c0020284914c05d47b0045d4bf0041b4914805d4ff004104924c05d4ff005104924c05d4ff0061f4924c05d4ff007194514c0504fb07872500020284d05005043b1b872500020284135005043b1b8504fb1b85053b0b81ae920001ae5400020f8330058fbe00250fbe004740000361a4bd0005053b0d81ae810001ae5400020f8330058fbe00250fbe0047400002e1a53d0005057b0f87258002028552580504bb17872580020284945805053b138a35154a1504bb1187250002028493500504fb1385053b158a35124e072480020284144805043b0987248002028414480504bb048724c0020284904c05fec100d5fed100e5043b0485047b19872480020284504805d43b00d5d47b00e5d4bf0081b4904805d4ff0041b493480264800001a487000504fb198394934d05d4ff0041b453440104524405f4570005047b1983b450490240000001af05000910000285ff100005ff110015ff120025ff130035ff3b0041aec5000910000201a43a0001a4790001a4be0005fec00005fec00015fec00025fed00031a43b000724c0020284504c01af51000920000201af9200059f050285d43c0005d47c0015d4bc0025d4fc0035defc004920000284af80000f383b0ce51358be57daa3b725fe44acdb2d880604e367199080b4379c41bb6ed0000000000000008000000000000001f000000000000000500000000000000040000000000000020de9090cb50e71c2588c773487d1da7066d0c719849a7e58dc8b6397a25c567c000000000a785fe65000000008e27706500000000fe9a7ef200000000ab64e5f2000000000000072c0000000000000774","salt":"0x0000000000000000000000000000000000000000000000000000000000000000","state":[["0xde9090cb50e71c2588c773487d1da7066d0c719849a7e58dc8b6397a25c567c0","0x000000000000000e000000000000000000000000000000000000000000000000"],["0xf383b0ce51358be57daa3b725fe44acdb2d880604e367199080b4379c41bb6ed","0x000000000000000a000000000000000000000000000000000000000000000000"]],"balances":[["0x0000000000000000000000000000000000000000000000000000000000000000","0x0000000000004e20"],["0x0101010101010101010101010101010101010101010101010101010101010101","0x0000000000002710"]],"tx_id":"0x0bd77bcc811eb73578dbf37057962974e447ea10066fe0ffb753f7628d70b436","output_index":"0x00","tx_pointer_block_height":"0x00000008","tx_pointer_tx_idx":"0x0000"}],"contract_state":[{"contract_id":"7f9bbb7d0bc2a9698e6dcceb693e9ef682b4a027fbbdf0a440845a01535736a3","key":"de9090cb50e71c2588c773487d1da7066d0c719849a7e58dc8b6397a25c567c0","value":"000000000000000e000000000000000000000000000000000000000000000000"},{"contract_id":"7f9bbb7d0bc2a9698e6dcceb693e9ef682b4a027fbbdf0a440845a01535736a3","key":"f383b0ce51358be57daa3b725fe44acdb2d880604e367199080b4379c41bb6ed","value":"000000000000000a000000000000000000000000000000000000000000000000"}],"contract_balance":[{"contract_id":"7f9bbb7d0bc2a9698e6dcceb693e9ef682b4a027fbbdf0a440845a01535736a3","asset_id":"0000000000000000000000000000000000000000000000000000000000000000","amount":20000},{"contract_id":"7f9bbb7d0bc2a9698e6dcceb693e9ef682b4a027fbbdf0a440845a01535736a3","asset_id":"0101010101010101010101010101010101010101010101010101010101010101","amount":10000}]}
        "#;

        let dir = tempfile::tempdir().unwrap();

        let state: StateConfig = serde_json::from_str(json_data).unwrap();
        for level in CompressionLevel::iter().rev() {
            let mut encoder = Encoder::parquet(dir.path(), level).unwrap();
            encoder.write_coins(state.coins.clone()).unwrap();
            encoder.write_messages(state.messages.clone()).unwrap();
            encoder.write_contracts(state.contracts.clone()).unwrap();
            encoder
                .write_contract_state(state.contract_state.clone())
                .unwrap();
            encoder
                .write_contract_balance(state.contract_balance.clone())
                .unwrap();
            encoder.close().unwrap();
        }
    }
}
