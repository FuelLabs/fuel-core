use crate::GenesisCommitment;
use fuel_core_storage::{
    MerkleRoot,
    tables::Coins,
};
use fuel_core_types::{
    entities::coins::coin::{
        Coin,
        CompressedCoin,
        CompressedCoinV1,
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
    fuel_vm::SecretKey,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::table_entry::TableEntry;

#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct CoinConfig {
    /// auto-generated if None
    pub tx_id: Bytes32,
    pub output_index: u16,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    pub tx_pointer_block_height: BlockHeight,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    /// The index of the originating tx within `tx_pointer_block_height`
    pub tx_pointer_tx_idx: u16,
    #[serde(flatten)]
    pub owner: Owner,
    pub amount: u64,
    pub asset_id: AssetId,
}

impl From<TableEntry<Coins>> for CoinConfig {
    fn from(value: TableEntry<Coins>) -> Self {
        CoinConfig {
            tx_id: *value.key.tx_id(),
            output_index: value.key.output_index(),
            tx_pointer_block_height: value.value.tx_pointer().block_height(),
            tx_pointer_tx_idx: value.value.tx_pointer().tx_index(),
            owner: (*value.value.owner()).into(),
            amount: *value.value.amount(),
            asset_id: *value.value.asset_id(),
        }
    }
}

impl From<CoinConfig> for TableEntry<Coins> {
    fn from(config: CoinConfig) -> Self {
        Self {
            key: UtxoId::new(config.tx_id, config.output_index),
            value: CompressedCoin::V1(CompressedCoinV1 {
                owner: config.owner.into(),
                amount: config.amount,
                asset_id: config.asset_id,
                tx_pointer: TxPointer::new(
                    config.tx_pointer_block_height,
                    config.tx_pointer_tx_idx,
                ),
            }),
        }
    }
}

impl CoinConfig {
    pub fn utxo_id(&self) -> UtxoId {
        UtxoId::new(self.tx_id, self.output_index)
    }

    pub fn tx_pointer(&self) -> TxPointer {
        TxPointer::new(self.tx_pointer_block_height, self.tx_pointer_tx_idx)
    }
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for CoinConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            tx_id: crate::Randomize::randomize(&mut rng),
            output_index: rng.r#gen(),
            tx_pointer_block_height: rng.r#gen(),
            tx_pointer_tx_idx: rng.r#gen(),
            owner: crate::Randomize::randomize(&mut rng),
            amount: rng.r#gen(),
            asset_id: crate::Randomize::randomize(&mut rng),
        }
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

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum Owner {
    #[serde(rename = "owner")]
    Address(Address),
    #[serde(rename = "owner_secret")]
    SecretKey(SecretKey),
}

impl Default for Owner {
    fn default() -> Self {
        Self::Address(Address::default())
    }
}

impl From<Owner> for Address {
    fn from(owner: Owner) -> Self {
        match owner {
            Owner::Address(address) => address,
            Owner::SecretKey(secret_key) => {
                Address::from(<[u8; Address::LEN]>::from(secret_key.public_key().hash()))
            }
        }
    }
}

impl From<Address> for Owner {
    fn from(address: Address) -> Self {
        Self::Address(address)
    }
}

impl From<SecretKey> for Owner {
    fn from(secret: SecretKey) -> Self {
        Self::SecretKey(secret)
    }
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for Owner {
    fn randomize(rng: impl rand::Rng) -> Self {
        Self::Address(Address::randomize(rng))
    }
}

#[cfg(feature = "test-helpers")]
pub mod coin_config_helpers {
    use crate::CoinConfig;
    use fuel_core_types::{
        fuel_types::Bytes32,
        fuel_vm::SecretKey,
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

        pub fn generate(&mut self) -> CoinConfig {
            let tx_id = tx_id(self.count);

            let config = CoinConfig {
                tx_id,
                output_index: self.count,
                ..Default::default()
            };

            self.count = self.count.checked_add(1).expect("Max coin count reached");

            config
        }

        pub fn generate_with(&mut self, secret: SecretKey, amount: u64) -> CoinConfig {
            CoinConfig {
                amount,
                owner: secret.into(),
                ..self.generate()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::fuel_vm::SecretKey;

    #[test]
    fn test_generate_unique_utxo_id() {
        let mut generator = coin_config_helpers::CoinConfigGenerator::new();
        let config1 = generator.generate();
        let config2 = generator.generate();

        assert_ne!(config1.utxo_id(), config2.utxo_id());
    }

    #[test]
    fn test_generate_with_owner_and_amount() {
        let mut rng = rand::thread_rng();
        let secret = SecretKey::random(&mut rng);
        let amount = 1000;

        let mut generator = coin_config_helpers::CoinConfigGenerator::new();
        let config = generator.generate_with(secret, amount);

        assert_eq!(config.owner, secret.into());
        assert_eq!(config.amount, amount);
    }
}
