use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
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
        self,
        Address,
        AssetId,
        TxId,
        UtxoId,
    },
    fuel_types::{
        self,
        Nonce,
    },
};

use crate::graphql_api::indexation;

use self::indexation::{
    coins_to_spend::{
        NON_RETRYABLE_BYTE,
        RETRYABLE_BYTE,
    },
    error::IndexationError,
};

const AMOUNT_SIZE: usize = size_of::<u64>();
const UTXO_ID_SIZE: usize = size_of::<UtxoId>();
const RETRYABLE_FLAG_SIZE: usize = size_of::<u8>();

// TODO: Reuse `fuel_vm::storage::double_key` macro.
pub fn owner_coin_id_key(owner: &Address, coin_id: &UtxoId) -> OwnedCoinKey {
    let mut default = [0u8; Address::LEN + UTXO_ID_SIZE];
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    let utxo_id_bytes: [u8; UTXO_ID_SIZE] = utxo_id_to_bytes(coin_id);
    default[Address::LEN..].copy_from_slice(utxo_id_bytes.as_ref());
    default
}

/// The storage table for the index of coins to spend.

// In the implementation of getters we use the explicit panic with the message (`expect`)
// when the key is malformed (incorrect length). This is a bit of a code smell, but it's
// consistent with how the `double_key!` macro works. We should consider refactoring this
// in the future.
pub struct CoinsToSpendIndex;

impl Mappable for CoinsToSpendIndex {
    type Key = Self::OwnedKey;
    type OwnedKey = CoinsToSpendIndexKey;
    type Value = Self::OwnedValue;
    type OwnedValue = IndexedCoinType;
}

impl TableWithBlueprint for CoinsToSpendIndex {
    type Blueprint = Plain<Raw, Raw>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::CoinsToSpend
    }
}

// For coins, the foreign key is the UtxoId (34 bytes).
pub(crate) const COIN_FOREIGN_KEY_LEN: usize = UTXO_ID_SIZE;

// For messages, the foreign key is the nonce (32 bytes).
pub(crate) const MESSAGE_FOREIGN_KEY_LEN: usize = Nonce::LEN;

#[repr(u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum IndexedCoinType {
    Coin,
    Message,
}

impl AsRef<[u8]> for IndexedCoinType {
    fn as_ref(&self) -> &[u8] {
        match self {
            IndexedCoinType::Coin => &[IndexedCoinType::Coin as u8],
            IndexedCoinType::Message => &[IndexedCoinType::Message as u8],
        }
    }
}

impl TryFrom<&[u8]> for IndexedCoinType {
    type Error = IndexationError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            [0] => Ok(IndexedCoinType::Coin),
            [1] => Ok(IndexedCoinType::Message),
            [] => Err(IndexationError::InvalidIndexedCoinType { coin_type: None }),
            x => Err(IndexationError::InvalidIndexedCoinType {
                coin_type: Some(x[0]),
            }),
        }
    }
}

pub type CoinsToSpendIndexEntry = (CoinsToSpendIndexKey, IndexedCoinType);

// TODO: Convert this key from Vec to strongly typed struct: https://github.com/FuelLabs/fuel-core/issues/2498
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CoinsToSpendIndexKey(Vec<u8>);

impl core::fmt::Display for CoinsToSpendIndexKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "retryable_flag={}, owner={}, asset_id={}, amount={}",
            self.retryable_flag(),
            self.owner(),
            self.asset_id(),
            self.amount()
        )
    }
}

impl TryFrom<&CoinsToSpendIndexKey> for fuel_tx::UtxoId {
    type Error = ();

    fn try_from(value: &CoinsToSpendIndexKey) -> Result<Self, Self::Error> {
        let bytes: [u8; COIN_FOREIGN_KEY_LEN] =
            value.foreign_key_bytes().try_into().map_err(|_| ())?;

        let (tx_id_bytes, output_index_bytes) = bytes.split_at(TxId::LEN);
        let tx_id = TxId::try_from(tx_id_bytes).map_err(|_| ())?;
        let output_index =
            u16::from_be_bytes(output_index_bytes.try_into().map_err(|_| ())?);
        Ok(fuel_tx::UtxoId::new(tx_id, output_index))
    }
}

impl TryFrom<&CoinsToSpendIndexKey> for fuel_types::Nonce {
    type Error = ();

    fn try_from(value: &CoinsToSpendIndexKey) -> Result<Self, Self::Error> {
        let bytes: [u8; MESSAGE_FOREIGN_KEY_LEN] =
            value.foreign_key_bytes().try_into().map_err(|_| ())?;
        Ok(fuel_types::Nonce::from(bytes))
    }
}

impl CoinsToSpendIndexKey {
    pub fn from_coin(coin: &Coin) -> Self {
        let retryable_flag_bytes = NON_RETRYABLE_BYTE;
        let address_bytes = coin.owner.as_ref();
        let asset_id_bytes = coin.asset_id.as_ref();
        let amount_bytes = coin.amount.to_be_bytes();
        let utxo_id_bytes = utxo_id_to_bytes(&coin.utxo_id);

        Self(
            retryable_flag_bytes
                .iter()
                .chain(address_bytes)
                .chain(asset_id_bytes)
                .chain(amount_bytes.iter())
                .chain(utxo_id_bytes.iter())
                .copied()
                .collect(),
        )
    }

    pub fn from_message(message: &Message, base_asset_id: &AssetId) -> Self {
        let retryable_flag_bytes = if message.is_retryable_message() {
            RETRYABLE_BYTE
        } else {
            NON_RETRYABLE_BYTE
        };
        let address_bytes = message.recipient().as_ref();
        let asset_id_bytes = base_asset_id.as_ref();
        let amount_bytes = message.amount().to_be_bytes();
        let nonce_bytes = message.nonce().as_slice();

        Self(
            retryable_flag_bytes
                .iter()
                .chain(address_bytes)
                .chain(asset_id_bytes)
                .chain(amount_bytes.iter())
                .chain(nonce_bytes)
                .copied()
                .collect(),
        )
    }

    fn from_slice(slice: &[u8]) -> Self {
        Self(slice.into())
    }

    pub fn owner(&self) -> Address {
        const ADDRESS_START: usize = RETRYABLE_FLAG_SIZE;
        const ADDRESS_END: usize = ADDRESS_START + Address::LEN;
        let address: [u8; Address::LEN] = self.0[ADDRESS_START..ADDRESS_END]
            .try_into()
            .expect("should have correct bytes");
        Address::new(address)
    }

    pub fn asset_id(&self) -> AssetId {
        const OFFSET: usize = RETRYABLE_FLAG_SIZE + Address::LEN;
        const ASSET_ID_START: usize = OFFSET;
        const ASSET_ID_END: usize = ASSET_ID_START + AssetId::LEN;
        let asset_id: [u8; AssetId::LEN] = self.0[ASSET_ID_START..ASSET_ID_END]
            .try_into()
            .expect("should have correct bytes");
        AssetId::new(asset_id)
    }

    pub fn retryable_flag(&self) -> u8 {
        const OFFSET: usize = 0;
        self.0[OFFSET]
    }

    pub fn amount(&self) -> u64 {
        const OFFSET: usize = RETRYABLE_FLAG_SIZE + Address::LEN + AssetId::LEN;
        const AMOUNT_START: usize = OFFSET;
        const AMOUNT_END: usize = AMOUNT_START + AMOUNT_SIZE;
        u64::from_be_bytes(
            self.0[AMOUNT_START..AMOUNT_END]
                .try_into()
                .expect("should have correct bytes"),
        )
    }

    pub fn foreign_key_bytes(&self) -> &[u8] {
        const OFFSET: usize =
            RETRYABLE_FLAG_SIZE + Address::LEN + AssetId::LEN + AMOUNT_SIZE;
        &self.0[OFFSET..]
    }
}

impl From<&[u8]> for CoinsToSpendIndexKey {
    fn from(slice: &[u8]) -> Self {
        CoinsToSpendIndexKey::from_slice(slice)
    }
}

impl AsRef<[u8]> for CoinsToSpendIndexKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
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
    use fuel_core_types::{
        entities::relayer::message::MessageV1,
        fuel_types::Nonce,
    };

    use super::*;

    impl rand::distributions::Distribution<CoinsToSpendIndexKey>
        for rand::distributions::Standard
    {
        fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> CoinsToSpendIndexKey {
            let bytes: Vec<_> = if rng.gen() {
                (0..COIN_TO_SPEND_COIN_KEY_LEN)
                    .map(|_| rng.gen::<u8>())
                    .collect()
            } else {
                (0..COIN_TO_SPEND_MESSAGE_KEY_LEN)
                    .map(|_| rng.gen::<u8>())
                    .collect()
            };
            CoinsToSpendIndexKey(bytes)
        }
    }

    // Base part of the coins to spend index key.
    const COIN_TO_SPEND_BASE_KEY_LEN: usize =
        RETRYABLE_FLAG_SIZE + Address::LEN + AssetId::LEN + AMOUNT_SIZE;

    // Total length of the coins to spend index key for coins.
    const COIN_TO_SPEND_COIN_KEY_LEN: usize =
        COIN_TO_SPEND_BASE_KEY_LEN + COIN_FOREIGN_KEY_LEN;

    // Total length of the coins to spend index key for messages.
    const COIN_TO_SPEND_MESSAGE_KEY_LEN: usize =
        COIN_TO_SPEND_BASE_KEY_LEN + MESSAGE_FOREIGN_KEY_LEN;

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
        <CoinsToSpendIndex as Mappable>::Key::default(),
        IndexedCoinType::Coin
    );

    fn merge_foreign_key_bytes<A, B, const N: usize>(a: A, b: B) -> [u8; N]
    where
        A: AsRef<[u8]>,
        B: AsRef<[u8]>,
    {
        a.as_ref()
            .iter()
            .copied()
            .chain(b.as_ref().iter().copied())
            .collect::<Vec<_>>()
            .try_into()
            .expect("should have correct length")
    }

    #[test]
    fn key_from_coin() {
        // Given
        let retryable_flag = NON_RETRYABLE_BYTE;

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
            key.as_ref().try_into().expect("should have correct length");

        assert_eq!(
            key_bytes,
            [
                0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
                0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
                0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22,
                0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E,
                0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A,
                0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46,
                0x47, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A,
                0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
                0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0xFE, 0xFF,
            ]
        );

        assert_eq!(key.owner(), owner);
        assert_eq!(key.asset_id(), asset_id);
        assert_eq!(key.retryable_flag(), retryable_flag[0]);
        assert_eq!(key.amount(), u64::from_be_bytes(amount));
        assert_eq!(
            key.foreign_key_bytes(),
            &merge_foreign_key_bytes::<_, _, COIN_FOREIGN_KEY_LEN>(tx_id, output_index)
        );
    }

    #[test]
    fn key_from_non_retryable_message() {
        // Given
        let retryable_flag = NON_RETRYABLE_BYTE;

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
            key.as_ref().try_into().expect("should have correct length");

        assert_eq!(
            key_bytes,
            [
                0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
                0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
                0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22,
                0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E,
                0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A,
                0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46,
                0x47, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A,
                0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
                0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F,
            ]
        );

        assert_eq!(key.owner(), owner);
        assert_eq!(key.asset_id(), base_asset_id);
        assert_eq!(key.retryable_flag(), retryable_flag[0]);
        assert_eq!(key.amount(), u64::from_be_bytes(amount));
        assert_eq!(key.foreign_key_bytes(), nonce.as_ref());
    }

    #[test]
    fn key_from_retryable_message() {
        // Given
        let retryable_flag = RETRYABLE_BYTE;

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
            key.as_ref().try_into().expect("should have correct length");

        assert_eq!(
            key_bytes,
            [
                0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
                0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
                0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22,
                0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E,
                0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A,
                0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46,
                0x47, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A,
                0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
                0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F
            ]
        );

        assert_eq!(key.owner(), owner);
        assert_eq!(key.asset_id(), base_asset_id);
        assert_eq!(key.retryable_flag(), retryable_flag[0]);
        assert_eq!(key.amount(), u64::from_be_bytes(amount));
        assert_eq!(key.foreign_key_bytes(), nonce.as_ref());
    }
}
