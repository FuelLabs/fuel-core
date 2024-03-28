use fuel_core_chain_config::{
    AddTable, AsTable, StateConfig, StateConfigBuilder, TableEntry,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{manual::Manual, postcard::Postcard, raw::Raw, Decode, Encode},
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    fuel_tx::{Address, Bytes32},
    fuel_types::BlockHeight,
    services::txpool::TransactionStatus,
};
use std::{array::TryFromSliceError, mem::size_of};

/// These tables allow iteration over all transactions owned by an address.
pub struct OwnedTransactions;

impl Mappable for OwnedTransactions {
    type Key = OwnedTransactionIndexKey;
    type OwnedKey = Self::Key;
    type Value = Bytes32;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for OwnedTransactions {
    type Blueprint = Plain<Manual<OwnedTransactionIndexKey>, Raw>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::TransactionsByOwnerBlockIdx
    }
}

impl AsTable<OwnedTransactions> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<OwnedTransactions>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<OwnedTransactions> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<OwnedTransactions>>) {
        // Do not include these for now
    }
}

/// The table stores the status of each transaction.
pub struct TransactionStatuses;

impl Mappable for TransactionStatuses {
    type Key = Bytes32;
    type OwnedKey = Self::Key;
    type Value = TransactionStatus;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TransactionStatuses {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::TransactionStatus
    }
}

impl AsTable<TransactionStatuses> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<TransactionStatuses>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<TransactionStatuses> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<TransactionStatuses>>) {
        // Do not include these for now
    }
}

const TX_INDEX_SIZE: usize = size_of::<TransactionIndex>();
const BLOCK_HEIGHT: usize = size_of::<BlockHeight>();
const INDEX_SIZE: usize = Address::LEN + BLOCK_HEIGHT + TX_INDEX_SIZE;

fn owned_tx_index_key(
    owner: &Address,
    height: BlockHeight,
    tx_idx: TransactionIndex,
) -> [u8; INDEX_SIZE] {
    let mut default = [0u8; INDEX_SIZE];
    // generate prefix to enable sorted indexing of transactions by owner
    // owner + block_height + tx_idx
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    default[Address::LEN..Address::LEN + BLOCK_HEIGHT]
        .copy_from_slice(height.to_bytes().as_ref());
    default[Address::LEN + BLOCK_HEIGHT..].copy_from_slice(tx_idx.to_be_bytes().as_ref());
    default
}

////////////////////////////////////// Not storage part //////////////////////////////////////

pub type TransactionIndex = u16;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct OwnedTransactionIndexKey {
    pub owner: Address,
    pub block_height: BlockHeight,
    pub tx_idx: TransactionIndex,
}

impl OwnedTransactionIndexKey {
    pub fn new(
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: TransactionIndex,
    ) -> Self {
        Self {
            owner: *owner,
            block_height,
            tx_idx,
        }
    }
}

impl From<[u8; INDEX_SIZE]> for OwnedTransactionIndexKey {
    fn from(bytes: [u8; INDEX_SIZE]) -> Self {
        let owner: [u8; 32] = bytes[..32].try_into().expect("It's an array of 32 bytes");
        // the first 32 bytes are the owner, which is already known when querying
        let mut block_height_bytes: [u8; 4] = Default::default();
        block_height_bytes.copy_from_slice(&bytes[32..36]);
        let mut tx_idx_bytes: [u8; 2] = Default::default();
        tx_idx_bytes.copy_from_slice(&bytes.as_ref()[36..38]);

        Self {
            owner: Address::from(owner),
            block_height: u32::from_be_bytes(block_height_bytes).into(),
            tx_idx: u16::from_be_bytes(tx_idx_bytes),
        }
    }
}

impl TryFrom<&[u8]> for OwnedTransactionIndexKey {
    type Error = TryFromSliceError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; INDEX_SIZE] = bytes.try_into()?;
        Ok(Self::from(bytes))
    }
}

impl Encode<OwnedTransactionIndexKey> for Manual<OwnedTransactionIndexKey> {
    type Encoder<'a> = [u8; INDEX_SIZE];

    fn encode(t: &OwnedTransactionIndexKey) -> Self::Encoder<'_> {
        owned_tx_index_key(&t.owner, t.block_height, t.tx_idx)
    }
}

impl Decode<OwnedTransactionIndexKey> for Manual<OwnedTransactionIndexKey> {
    fn decode(bytes: &[u8]) -> anyhow::Result<OwnedTransactionIndexKey> {
        OwnedTransactionIndexKey::try_from(bytes)
            .map_err(|_| anyhow::anyhow!("Unable to decode bytes"))
    }
}

#[derive(Clone, Debug, PartialOrd, Eq, PartialEq)]
pub struct OwnedTransactionIndexCursor {
    pub block_height: BlockHeight,
    pub tx_idx: TransactionIndex,
}

impl From<OwnedTransactionIndexKey> for OwnedTransactionIndexCursor {
    fn from(key: OwnedTransactionIndexKey) -> Self {
        OwnedTransactionIndexCursor {
            block_height: key.block_height,
            tx_idx: key.tx_idx,
        }
    }
}

impl From<Vec<u8>> for OwnedTransactionIndexCursor {
    fn from(bytes: Vec<u8>) -> Self {
        let mut block_height_bytes: [u8; 4] = Default::default();
        block_height_bytes.copy_from_slice(&bytes[..4]);
        let mut tx_idx_bytes: [u8; 2] = Default::default();
        tx_idx_bytes.copy_from_slice(&bytes[4..6]);

        Self {
            block_height: u32::from_be_bytes(block_height_bytes).into(),
            tx_idx: u16::from_be_bytes(tx_idx_bytes),
        }
    }
}

impl From<OwnedTransactionIndexCursor> for Vec<u8> {
    fn from(cursor: OwnedTransactionIndexCursor) -> Self {
        let mut bytes = Vec::with_capacity(8);
        bytes.extend(cursor.block_height.to_bytes());
        bytes.extend(cursor.tx_idx.to_be_bytes());
        bytes
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn generate_key(rng: &mut impl rand::Rng) -> <OwnedTransactions as Mappable>::Key {
        let mut bytes = [0u8; INDEX_SIZE];
        rng.fill(bytes.as_mut());
        bytes.into()
    }

    fuel_core_storage::basic_storage_tests!(
        OwnedTransactions,
        [1u8; INDEX_SIZE].into(),
        <OwnedTransactions as Mappable>::Value::default(),
        <OwnedTransactions as Mappable>::Value::default(),
        generate_key
    );

    fuel_core_storage::basic_storage_tests!(
        TransactionStatuses,
        <TransactionStatuses as Mappable>::Key::default(),
        TransactionStatus::Submitted {
            time: fuel_core_types::tai64::Tai64::UNIX_EPOCH,
        }
    );
}
