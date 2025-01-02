mod codecs;

use crate::fuel_core_graphql_api::{
    indexation::coins_to_spend::{
        NON_RETRYABLE_BYTE,
        RETRYABLE_BYTE,
    },
    storage::coins::codecs::UTXO_ID_SIZE,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        manual::Manual,
        postcard::Postcard,
        primitive::utxo_id_to_bytes,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    entities::{
        coins::coin::Coin,
        Message,
    },
    fuel_tx::{
        Address,
        AssetId,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
};

pub fn owner_coin_id_key(owner: &Address, coin_id: &UtxoId) -> OwnedCoinKey {
    let mut default = [0u8; Address::LEN + UTXO_ID_SIZE];
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    let utxo_id_bytes: [u8; UTXO_ID_SIZE] = utxo_id_to_bytes(coin_id);
    default[Address::LEN..].copy_from_slice(utxo_id_bytes.as_ref());
    default
}

/// The storage table for the index of coins to spend.
pub struct CoinsToSpendIndex;

impl Mappable for CoinsToSpendIndex {
    type Key = Self::OwnedKey;
    type OwnedKey = CoinsToSpendIndexKey;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for CoinsToSpendIndex {
    type Blueprint = Plain<Manual<CoinsToSpendIndexKey>, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::CoinsToSpend
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoinsToSpendIndexKey {
    Coin {
        owner: Address,
        asset_id: AssetId,
        amount: u64,
        utxo_id: UtxoId,
    },
    Message {
        retryable_flag: u8,
        owner: Address,
        asset_id: AssetId,
        amount: u64,
        nonce: Nonce,
    },
}

impl core::fmt::Display for CoinsToSpendIndexKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "retryable_flag={:?}, owner={:?}, asset_id={:?}, amount={:?}",
            self.retryable_flag(),
            self.owner(),
            self.asset_id(),
            self.amount()
        )
    }
}

impl CoinsToSpendIndexKey {
    pub fn from_coin(coin: &Coin) -> Self {
        Self::Coin {
            owner: coin.owner,
            asset_id: coin.asset_id,
            amount: coin.amount,
            utxo_id: coin.utxo_id,
        }
    }

    pub fn from_message(message: &Message, base_asset_id: &AssetId) -> Self {
        let retryable_flag_bytes = if message.is_retryable_message() {
            RETRYABLE_BYTE
        } else {
            NON_RETRYABLE_BYTE
        };
        Self::Message {
            retryable_flag: retryable_flag_bytes[0],
            owner: *message.recipient(),
            asset_id: *base_asset_id,
            amount: message.amount(),
            nonce: *message.nonce(),
        }
    }

    pub fn owner(&self) -> &Address {
        match self {
            CoinsToSpendIndexKey::Coin { owner, .. } => owner,
            CoinsToSpendIndexKey::Message { owner, .. } => owner,
        }
    }

    pub fn asset_id(&self) -> &AssetId {
        match self {
            CoinsToSpendIndexKey::Coin { asset_id, .. } => asset_id,
            CoinsToSpendIndexKey::Message { asset_id, .. } => asset_id,
        }
    }

    pub fn retryable_flag(&self) -> u8 {
        match self {
            CoinsToSpendIndexKey::Coin { .. } => NON_RETRYABLE_BYTE[0],
            CoinsToSpendIndexKey::Message { retryable_flag, .. } => *retryable_flag,
        }
    }

    pub fn amount(&self) -> u64 {
        match self {
            CoinsToSpendIndexKey::Coin { amount, .. } => *amount,
            CoinsToSpendIndexKey::Message { amount, .. } => *amount,
        }
    }
}

/// The storage table of owned coin ids. Maps addresses to owned coins.
pub struct OwnedCoins;
/// The storage key for owned coins: `Address ++ UtxoId`
pub type OwnedCoinKey = [u8; Address::LEN + TxId::LEN + 2];

impl Mappable for OwnedCoins {
    type Key = Self::OwnedKey;
    type OwnedKey = OwnedCoinKey;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for OwnedCoins {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::OwnedCoins
    }
}

#[cfg(test)]
mod test {
    use crate::{
        fuel_core_graphql_api::storage::coins::codecs::{
            AMOUNT_SIZE,
            COIN_TYPE_SIZE,
            RETRYABLE_FLAG_SIZE,
        },
        graphql_api::storage::coins::codecs::CoinType,
    };
    use fuel_core_storage::codec::{
        Encode,
        Encoder,
    };
    use fuel_core_types::{
        entities::relayer::message::MessageV1,
        fuel_types::Nonce,
    };

    use super::*;

    impl rand::distributions::Distribution<CoinsToSpendIndexKey>
        for rand::distributions::Standard
    {
        fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> CoinsToSpendIndexKey {
            if rng.gen() {
                CoinsToSpendIndexKey::Coin {
                    owner: rng.gen(),
                    asset_id: rng.gen(),
                    amount: rng.gen(),
                    utxo_id: rng.gen(),
                }
            } else {
                CoinsToSpendIndexKey::Message {
                    retryable_flag: rng.gen(),
                    owner: rng.gen(),
                    asset_id: rng.gen(),
                    amount: rng.gen(),
                    nonce: rng.gen(),
                }
            }
        }
    }

    // Base part of the coins to spend index key.
    const COIN_TO_SPEND_BASE_KEY_LEN: usize =
        RETRYABLE_FLAG_SIZE + Address::LEN + AssetId::LEN + AMOUNT_SIZE;

    // Total length of the coins to spend index key for coins.
    const COIN_TO_SPEND_COIN_KEY_LEN: usize =
        COIN_TO_SPEND_BASE_KEY_LEN + UTXO_ID_SIZE + COIN_TYPE_SIZE;

    // Total length of the coins to spend index key for messages.
    const COIN_TO_SPEND_MESSAGE_KEY_LEN: usize =
        COIN_TO_SPEND_BASE_KEY_LEN + Nonce::LEN + COIN_TYPE_SIZE;

    fn generate_key(rng: &mut impl rand::Rng) -> <OwnedCoins as Mappable>::Key {
        let mut bytes = [0u8; 66];
        rng.fill(bytes.as_mut());
        bytes
    }

    fuel_core_storage::basic_storage_tests!(
        OwnedCoins,
        [0u8; 66],
        <OwnedCoins as Mappable>::Value::default(),
        <OwnedCoins as Mappable>::Value::default(),
        generate_key
    );

    fuel_core_storage::basic_storage_tests!(
        CoinsToSpendIndex,
        CoinsToSpendIndexKey::Coin {
            owner: Default::default(),
            asset_id: Default::default(),
            amount: 0,
            utxo_id: Default::default(),
        },
        Default::default()
    );

    #[test]
    fn serialized_key_from_coin_is_correct() {
        // Given
        let retryable_flag = NON_RETRYABLE_BYTE[0];

        let owner = Address::new([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
            0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
            0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
        ]);

        let asset_id = AssetId::new([
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C,
            0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
            0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F,
        ]);

        let amount = [0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47];
        assert_eq!(amount.len(), AMOUNT_SIZE);

        let tx_id = TxId::new([
            0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C,
            0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
            0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F,
        ]);

        let output_index = [0xFE, 0xFF];
        let utxo_id = UtxoId::new(tx_id, u16::from_be_bytes(output_index));

        let coin = Coin {
            owner,
            asset_id,
            amount: u64::from_be_bytes(amount),
            utxo_id,
            tx_pointer: Default::default(),
        };

        // When
        let key = CoinsToSpendIndexKey::from_coin(&coin);

        // Then
        let key_bytes: [u8; COIN_TO_SPEND_COIN_KEY_LEN] =
            Manual::<CoinsToSpendIndexKey>::encode(&key)
                .as_bytes()
                .as_ref()
                .try_into()
                .expect("should have correct length");

        #[rustfmt::skip]
        assert_eq!(
            key_bytes,
            [
                retryable_flag, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
                0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
                0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22,
                0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E,
                0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A,
                0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46,
                0x47, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A,
                0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
                0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0xFE, 0xFF, CoinType::Coin as u8,
            ]
        );
    }

    #[test]
    fn serialized_key_from_non_retryable_message_is_correct() {
        // Given
        let retryable_flag = NON_RETRYABLE_BYTE[0];

        let owner = Address::new([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
            0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
            0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
        ]);

        let base_asset_id = AssetId::new([
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C,
            0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
            0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F,
        ]);

        let amount = [0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47];
        assert_eq!(amount.len(), AMOUNT_SIZE);

        let nonce = Nonce::new([
            0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C,
            0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
            0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F,
        ]);

        let message = Message::V1(MessageV1 {
            recipient: owner,
            amount: u64::from_be_bytes(amount),
            nonce,
            sender: Default::default(),
            data: vec![],
            da_height: Default::default(),
        });

        // When
        let key = CoinsToSpendIndexKey::from_message(&message, &base_asset_id);

        // Then
        let key_bytes: [u8; COIN_TO_SPEND_MESSAGE_KEY_LEN] =
            Manual::<CoinsToSpendIndexKey>::encode(&key)
                .as_bytes()
                .as_ref()
                .try_into()
                .expect("should have correct length");

        #[rustfmt::skip]
        assert_eq!(
            key_bytes,
            [
                retryable_flag, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
                0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
                0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22,
                0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E,
                0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A,
                0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46,
                0x47, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A,
                0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
                0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, CoinType::Message as u8,
            ]
        );
    }

    #[test]
    fn serialized_key_from_retryable_message_is_correct() {
        // Given
        let retryable_flag = RETRYABLE_BYTE[0];

        let owner = Address::new([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
            0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
            0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
        ]);

        let base_asset_id = AssetId::new([
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C,
            0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
            0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F,
        ]);

        let amount = [0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47];
        assert_eq!(amount.len(), AMOUNT_SIZE);

        let nonce = Nonce::new([
            0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C,
            0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
            0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F,
        ]);

        let message = Message::V1(MessageV1 {
            recipient: owner,
            amount: u64::from_be_bytes(amount),
            nonce,
            sender: Default::default(),
            data: vec![1],
            da_height: Default::default(),
        });

        // When
        let key = CoinsToSpendIndexKey::from_message(&message, &base_asset_id);

        // Then
        let key_bytes: [u8; COIN_TO_SPEND_MESSAGE_KEY_LEN] =
            Manual::<CoinsToSpendIndexKey>::encode(&key)
                .as_bytes()
                .as_ref()
                .try_into()
                .expect("should have correct length");

        #[rustfmt::skip]
        assert_eq!(
            key_bytes,
            [
                retryable_flag, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
                0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
                0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22,
                0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E,
                0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A,
                0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46,
                0x47, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A,
                0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
                0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, CoinType::Message as u8,
            ]
        );
    }
}
