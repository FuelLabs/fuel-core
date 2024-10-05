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
    pub owner: Address,
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
            owner: *value.value.owner(),
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
                owner: config.owner,
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

/// Generates a new coin config with a unique utxo id for testing
#[derive(Default, Debug)]
pub struct CoinConfigGenerator {
    count: u16,
}

impl CoinConfigGenerator {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn generate(&mut self) -> CoinConfig {
        let mut bytes = [0u8; 32];
        bytes[..std::mem::size_of_val(&self.count)]
            .copy_from_slice(&self.count.to_be_bytes());

        let config = CoinConfig {
            tx_id: Bytes32::from(bytes),
            tx_pointer_tx_idx: self.count,
            ..Default::default()
        };
        self.count = self.count.checked_add(1).expect("Max coin count reached");

        config
    }

    pub fn generate_with(&mut self, secret: SecretKey, amount: u64) -> CoinConfig {
        let owner = Address::from(*secret.public_key().hash());

        CoinConfig {
            amount,
            owner,
            ..self.generate()
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
            output_index: rng.gen(),
            tx_pointer_block_height: rng.gen(),
            tx_pointer_tx_idx: rng.gen(),
            owner: crate::Randomize::randomize(&mut rng),
            amount: rng.gen(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::{
        fuel_types::Address,
        fuel_vm::SecretKey,
    };

    #[test]
    fn test_generate_unique_utxo_id() {
        let mut generator = CoinConfigGenerator::new();
        let config1 = generator.generate();
        let config2 = generator.generate();

        assert_ne!(config1.utxo_id(), config2.utxo_id());
    }

    #[test]
    fn test_generate_with_owner_and_amount() {
        let mut rng = rand::thread_rng();
        let secret = SecretKey::random(&mut rng);
        let amount = 1000;

        let mut generator = CoinConfigGenerator::new();
        let config = generator.generate_with(secret, amount);

        assert_eq!(config.owner, Address::from(*secret.public_key().hash()));
        assert_eq!(config.amount, amount);
    }
}
