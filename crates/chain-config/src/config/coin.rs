use crate::GenesisCommitment;
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    entities::coins::coin::CompressedCoin,
    fuel_crypto::Hasher,
    fuel_tx::{
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        bytes::WORD_SIZE,
        Address,
        AssetId,
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

#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct CoinConfig {
    /// auto-generated if None
    pub tx_id: Bytes32,
    pub output_index: u8,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    pub tx_pointer_block_height: BlockHeight,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    /// The index of the originating tx within `tx_pointer_block_height`
    pub tx_pointer_tx_idx: u16,
    pub owner: Address,
    pub amount: u64,
    pub asset_id: AssetId,
}

/// Generates a new coin config with a unique utxo id for testing
pub struct CoinConfigGenerator {
    count: usize,
}

impl CoinConfigGenerator {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn generate(&mut self) -> CoinConfig {
        // generated transaction id([0..[count/255]])
        let tx_id = Bytes32::try_from(
            (0..(Bytes32::LEN - WORD_SIZE))
                .map(|_| 0u8)
                .chain((self.count as u64 / 255).to_be_bytes().into_iter())
                .collect_vec()
                .as_slice(),
        )
        .expect("Incorrect transaction id byte length");

        let config = CoinConfig {
            tx_id,
            output_index: self.count as u8,
            ..Default::default()
        };
        self.count += 1;

        config
    }

    pub fn generate_with(&mut self, secret: SecretKey, amount: u64) -> CoinConfig {
        let owner = Address::from(*secret.public_key().hash());

        let config = CoinConfig {
            amount,
            owner,
            ..self.generate()
        };

        config
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

#[cfg(all(test, feature = "random", feature = "std"))]
impl crate::Randomize for CoinConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            tx_id: super::random_bytes_32(&mut rng).into(),
            output_index: rng.gen(),
            tx_pointer_block_height: rng.gen(),
            tx_pointer_tx_idx: rng.gen(),
            owner: Address::new(super::random_bytes_32(&mut rng)),
            amount: rng.gen(),
            asset_id: AssetId::new(super::random_bytes_32(rng)),
        }
    }
}

impl GenesisCommitment for CompressedCoin {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let owner = self.owner();
        let amount = self.amount();
        let asset_id = self.asset_id();
        let tx_pointer = self.tx_pointer();

        let coin_hash = *Hasher::default()
            .chain(owner)
            .chain(amount.to_be_bytes())
            .chain(asset_id)
            .chain(tx_pointer.block_height().to_be_bytes())
            .chain(tx_pointer.tx_index().to_be_bytes())
            .finalize();

        Ok(coin_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::{
        fuel_types::{
            Address,
            Bytes32,
        },
        fuel_vm::SecretKey,
    };

    #[test]
    fn test_coin_config_generator() {
        let mut generator = CoinConfigGenerator::new();
        let coin = generator.generate();
        assert_eq!(coin.tx_id, Bytes32::from([0u8; 32]));
        assert_eq!(coin.output_index, 0);

        let coin = generator.generate();
        assert_eq!(coin.tx_id, Bytes32::from([0u8; 32]));
        assert_eq!(coin.output_index, 1);

        let coin = generator.generate();
        assert_eq!(coin.tx_id, Bytes32::from([0u8; 32]));
        assert_eq!(coin.output_index, 2);
    }

    #[test]
    fn test_coin_config_generator_with() {
        let mut rng = rand::thread_rng();
        let mut generator = CoinConfigGenerator::new();
        let secret = SecretKey::random(&mut rng);

        let coin = generator.generate_with(secret, 100);

        assert_eq!(coin.tx_id, Bytes32::from([0u8; 32]));
        assert_eq!(coin.output_index, 0);
        assert_eq!(coin.owner, Address::from(*secret.public_key().hash()));
        assert_eq!(coin.amount, 100);
    }
}
