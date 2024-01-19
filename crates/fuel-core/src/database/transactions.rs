use crate::database::{
    Column,
    Database,
};
use core::{
    array::TryFromSliceError,
    mem::size_of,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        manual::Manual,
        postcard::Postcard,
        raw::Raw,
        Decode,
        Encode,
    },
    iter::IterDirection,
    structured_storage::TableWithBlueprint,
    tables::Transactions,
    Mappable,
    Result as StorageResult,
};
use fuel_core_types::{
    self,
    fuel_tx::{
        Bytes32,
        Transaction,
        TxPointer,
    },
    fuel_types::{
        Address,
        BlockHeight,
    },
    services::txpool::TransactionStatus,
};

/// Teh tables allows to iterate over all transactions owned by an address.
pub struct OwnedTransactions;

impl Mappable for OwnedTransactions {
    type Key = OwnedTransactionIndexKey;
    type OwnedKey = Self::Key;
    type Value = Bytes32;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for OwnedTransactions {
    type Blueprint = Plain<Manual<OwnedTransactionIndexKey>, Raw>;

    fn column() -> Column {
        Column::TransactionsByOwnerBlockIdx
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

    fn column() -> Column {
        Column::TransactionStatus
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

impl Database {
    pub fn all_transactions(
        &self,
        start: Option<&Bytes32>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<Transaction>> + '_ {
        let start = start.map(|b| b.as_ref().to_vec());
        self.iter_all_by_start::<Transactions, _>(start, direction)
            .map(|res| res.map(|(_, tx)| tx))
    }

    /// Iterates over a KV mapping of `[address + block height + tx idx] => transaction id`. This
    /// allows for efficient lookup of transaction ids associated with an address, sorted by
    /// block age and ordering within a block. The cursor tracks the `[block height + tx idx]` for
    /// pagination purposes.
    pub fn owned_transactions(
        &self,
        owner: Address,
        start: Option<OwnedTransactionIndexCursor>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(TxPointer, Bytes32)>> + '_ {
        let start = start
            .map(|cursor| owned_tx_index_key(&owner, cursor.block_height, cursor.tx_idx));
        self.iter_all_filtered::<OwnedTransactions, _, _>(Some(owner), start, direction)
            .map(|res| {
                res.map(|(key, tx_id)| {
                    (TxPointer::new(key.block_height, key.tx_idx), tx_id)
                })
            })
    }

    pub fn record_tx_id_owner(
        &mut self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: TransactionIndex,
        tx_id: &Bytes32,
    ) -> StorageResult<Option<Bytes32>> {
        use fuel_core_storage::StorageAsMut;
        self.storage::<OwnedTransactions>().insert(
            &OwnedTransactionIndexKey::new(owner, block_height, tx_idx),
            tx_id,
        )
    }

    pub fn update_tx_status(
        &mut self,
        id: &Bytes32,
        status: TransactionStatus,
    ) -> StorageResult<Option<TransactionStatus>> {
        use fuel_core_storage::StorageAsMut;
        self.storage::<TransactionStatuses>().insert(id, &status)
    }

    pub fn get_tx_status(
        &self,
        id: &Bytes32,
    ) -> StorageResult<Option<TransactionStatus>> {
        use fuel_core_storage::StorageAsRef;
        self.storage::<TransactionStatuses>()
            .get(id)
            .map(|v| v.map(|v| v.into_owned()))
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

#[derive(Clone)]
pub struct OwnedTransactionIndexKey {
    owner: Address,
    block_height: BlockHeight,
    tx_idx: TransactionIndex,
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
