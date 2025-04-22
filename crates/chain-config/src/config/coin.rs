use serde::{
    Deserialize,
    Serialize,
};

use crate::GenesisCommitment;
use fuel_core_storage::{
    tables::Coins,
    MerkleRoot,
};
use fuel_core_types::{
    entities::coins::coin::{
        Coin,
        CompressedCoin,
        CompressedCoinV1,
        CompressedCoinV2,
        DataCoin,
    },
    fuel_crypto::Hasher,
    fuel_tx::{
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        BlockHeight,
        Bytes32,
    },
};

use super::table_entry::TableEntry;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum CoinConfig {
    Coin(ConfigCoin),
    DataCoin(ConfigDataCoin),
}

#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ConfigCoin {
    pub tx_id: Bytes32,
    pub output_index: u16,
    pub tx_pointer_block_height: BlockHeight,
    pub tx_pointer_tx_idx: u16,
    pub owner: Address,
    pub amount: u64,
    pub asset_id: AssetId,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ConfigDataCoin {
    pub tx_id: Bytes32,
    pub output_index: u16,
    pub tx_pointer_block_height: BlockHeight,
    pub tx_pointer_tx_idx: u16,
    pub owner: Address,
    pub amount: u64,
    pub asset_id: AssetId,
    pub data: Vec<u8>,
}

impl CoinConfig {
    pub fn tx_id(&self) -> Bytes32 {
        match self {
            CoinConfig::Coin(ConfigCoin { tx_id, .. }) => *tx_id,
            CoinConfig::DataCoin(ConfigDataCoin { tx_id, .. }) => *tx_id,
        }
    }

    pub fn output_index(&self) -> u16 {
        match self {
            CoinConfig::Coin(ConfigCoin { output_index, .. }) => *output_index,
            CoinConfig::DataCoin(ConfigDataCoin { output_index, .. }) => *output_index,
        }
    }

    pub fn mut_output_index(&mut self) -> &mut u16 {
        match self {
            CoinConfig::Coin(ConfigCoin { output_index, .. }) => output_index,
            CoinConfig::DataCoin(ConfigDataCoin { output_index, .. }) => output_index,
        }
    }

    pub fn tx_pointer_block_height(&self) -> BlockHeight {
        match self {
            CoinConfig::Coin(ConfigCoin {
                tx_pointer_block_height,
                ..
            }) => *tx_pointer_block_height,
            CoinConfig::DataCoin(ConfigDataCoin {
                tx_pointer_block_height,
                ..
            }) => *tx_pointer_block_height,
        }
    }

    pub fn tx_pointer_tx_idx(&self) -> u16 {
        match self {
            CoinConfig::Coin(ConfigCoin {
                tx_pointer_tx_idx, ..
            }) => *tx_pointer_tx_idx,
            CoinConfig::DataCoin(ConfigDataCoin {
                tx_pointer_tx_idx, ..
            }) => *tx_pointer_tx_idx,
        }
    }

    pub fn owner(&self) -> Address {
        match self {
            CoinConfig::Coin(ConfigCoin { owner, .. }) => *owner,
            CoinConfig::DataCoin(ConfigDataCoin { owner, .. }) => *owner,
        }
    }

    pub fn mut_owner(&mut self) -> &mut Address {
        match self {
            CoinConfig::Coin(ConfigCoin { owner, .. }) => owner,
            CoinConfig::DataCoin(ConfigDataCoin { owner, .. }) => owner,
        }
    }

    pub fn amount(&self) -> u64 {
        match self {
            CoinConfig::Coin(ConfigCoin { amount, .. }) => *amount,
            CoinConfig::DataCoin(ConfigDataCoin { amount, .. }) => *amount,
        }
    }

    pub fn asset_id(&self) -> AssetId {
        match self {
            CoinConfig::Coin(ConfigCoin { asset_id, .. }) => *asset_id,
            CoinConfig::DataCoin(ConfigDataCoin { asset_id, .. }) => *asset_id,
        }
    }

    pub fn mut_asset_id(&mut self) -> &mut AssetId {
        match self {
            CoinConfig::Coin(ConfigCoin { asset_id, .. }) => asset_id,
            CoinConfig::DataCoin(ConfigDataCoin { asset_id, .. }) => asset_id,
        }
    }

    pub fn data(&self) -> Option<&Vec<u8>> {
        match self {
            CoinConfig::Coin(_) => None,
            CoinConfig::DataCoin(ConfigDataCoin { data, .. }) => Some(data),
        }
    }
}

impl From<ConfigCoin> for CoinConfig {
    fn from(value: ConfigCoin) -> Self {
        CoinConfig::Coin(value)
    }
}

impl From<ConfigDataCoin> for CoinConfig {
    fn from(value: ConfigDataCoin) -> Self {
        CoinConfig::DataCoin(value)
    }
}

impl From<TableEntry<Coins>> for CoinConfig {
    fn from(value: TableEntry<Coins>) -> Self {
        match value.value {
            CompressedCoin::V1(coin) => CoinConfig::Coin(ConfigCoin {
                tx_id: *value.key.tx_id(),
                output_index: value.key.output_index(),
                tx_pointer_block_height: coin.tx_pointer.block_height(),
                tx_pointer_tx_idx: coin.tx_pointer.tx_index(),
                owner: coin.owner,
                amount: coin.amount,
                asset_id: coin.asset_id,
            }),
            CompressedCoin::V2(data_coin) => {
                CoinConfig::DataCoin(ConfigDataCoin {
                    tx_id: *value.key.tx_id(),
                    output_index: value.key.output_index(),
                    tx_pointer_block_height: data_coin.tx_pointer.block_height(),
                    tx_pointer_tx_idx: data_coin.tx_pointer.tx_index(),
                    owner: data_coin.owner,
                    amount: data_coin.amount,
                    asset_id: data_coin.asset_id,
                    data: data_coin.data.clone(), // Clone the data for the new struct
                })
            }
            _ => {
                unreachable!("Covered both variants")
            }
        }
    }
}

impl From<CoinConfig> for TableEntry<Coins> {
    fn from(config: CoinConfig) -> Self {
        let entry = match config {
            CoinConfig::Coin(config) => {
                let value = CompressedCoin::V1(CompressedCoinV1 {
                    owner: config.owner,
                    amount: config.amount,
                    asset_id: config.asset_id,
                    tx_pointer: TxPointer::new(
                        config.tx_pointer_block_height,
                        config.tx_pointer_tx_idx,
                    ),
                });
                Self {
                    key: UtxoId::new(config.tx_id, config.output_index),
                    value,
                }
            }
            CoinConfig::DataCoin(config) => {
                let value = CompressedCoin::V2(CompressedCoinV2 {
                    owner: config.owner,
                    amount: config.amount,
                    asset_id: config.asset_id,
                    tx_pointer: TxPointer::new(
                        config.tx_pointer_block_height,
                        config.tx_pointer_tx_idx,
                    ),
                    data: config.data,
                });

                Self {
                    key: UtxoId::new(config.tx_id, config.output_index),
                    value,
                }
            }
        };
        // tracing::debug!(
        //     "Created TableEntry: key={:?}, value={:?}",
        //     &entry.key,
        //     &entry.value,
        // );
        entry
    }
}

impl CoinConfig {
    pub fn utxo_id(&self) -> UtxoId {
        UtxoId::new(self.tx_id(), self.output_index())
    }

    pub fn tx_pointer(&self) -> TxPointer {
        TxPointer::new(self.tx_pointer_block_height(), self.tx_pointer_tx_idx())
    }
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for ConfigCoin {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        ConfigCoin {
            tx_id: crate::Randomize::randomize(&mut rng),
            output_index: rng.gen(),
            tx_pointer_block_height: rng.gen(),
            tx_pointer_tx_idx: rng.gen(),
            owner: crate::Randomize::randomize(&mut rng),
            amount: rng.gen(),
            asset_id: crate::Randomize::randomize(&mut rng),
        }
    }
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for CoinConfig {
    fn randomize(rng: impl ::rand::Rng) -> Self {
        ConfigCoin::randomize(rng).into()
    }
}

impl GenesisCommitment for Coin {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let owner = self.owner;
        let amount = self.amount;
        let asset_id = self.asset_id;
        let tx_pointer = self.tx_pointer;
        let utxo_id = self.utxo_id;

        let coin_hash = *Hasher::default()
            .chain(owner)
            .chain(amount.to_be_bytes())
            .chain(asset_id)
            .chain(tx_pointer.block_height().to_be_bytes())
            .chain(tx_pointer.tx_index().to_be_bytes())
            .chain(utxo_id.tx_id())
            .chain(utxo_id.output_index().to_be_bytes())
            .finalize();

        Ok(coin_hash)
    }
}

impl GenesisCommitment for DataCoin {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let owner = self.owner;
        let amount = self.amount;
        let asset_id = self.asset_id;
        let tx_pointer = self.tx_pointer;
        let utxo_id = self.utxo_id;
        let data = &self.data;

        let coin_hash = *Hasher::default()
            .chain(owner)
            .chain(amount.to_be_bytes())
            .chain(asset_id)
            .chain(tx_pointer.block_height().to_be_bytes())
            .chain(tx_pointer.tx_index().to_be_bytes())
            .chain(utxo_id.tx_id())
            .chain(utxo_id.output_index().to_be_bytes())
            .chain(data)
            .finalize();

        Ok(coin_hash)
    }
}

#[cfg(feature = "test-helpers")]
pub mod coin_config_helpers {
    use fuel_core_types::{
        fuel_types::{
            Address,
            Bytes32,
        },
        fuel_vm::SecretKey,
    };

    use crate::{
        CoinConfig,
        ConfigCoin,
    };

    type CoinCount = u16;

    /// Generates a new coin config with a unique utxo id for testing
    #[derive(Default, Debug)]
    pub struct CoinConfigGenerator {
        count: CoinCount,
    }

    pub fn tx_id(count: CoinCount) -> Bytes32 {
        let mut bytes = [0u8; 32];
        bytes[..size_of::<CoinCount>()].copy_from_slice(&count.to_be_bytes());
        bytes.into()
    }

    impl CoinConfigGenerator {
        pub fn new() -> Self {
            Self { count: 0 }
        }

        pub fn generate(&mut self) -> ConfigCoin {
            let tx_id = tx_id(self.count);

            let config = ConfigCoin {
                tx_id,
                output_index: self.count,
                ..Default::default()
            };

            self.count = self.count.checked_add(1).expect("Max coin count reached");

            config
        }

        pub fn generate_with(&mut self, secret: SecretKey, amount: u64) -> CoinConfig {
            let owner = Address::from(*secret.public_key().hash());

            ConfigCoin {
                amount,
                owner,
                ..self.generate()
            }
            .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use fuel_core_types::{
        fuel_types::Address,
        fuel_vm::SecretKey,
    };

    use super::*;

    #[test]
    fn test_generate_unique_utxo_id() {
        let mut generator = coin_config_helpers::CoinConfigGenerator::new();
        let config1 = generator.generate();
        let config2 = generator.generate();

        assert_ne!(config1.tx_id, config2.tx_id);
    }

    #[test]
    fn test_generate_with_owner_and_amount() {
        let mut rng = rand::thread_rng();
        let secret = SecretKey::random(&mut rng);
        let amount = 1000;

        let mut generator = coin_config_helpers::CoinConfigGenerator::new();
        let config = generator.generate_with(secret, amount);

        assert_eq!(config.owner(), Address::from(*secret.public_key().hash()));
        assert_eq!(config.amount(), amount);
    }
}
