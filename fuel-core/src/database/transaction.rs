use fuel_core_interfaces::model::BlockHeight;

pub type TransactionIndex = u32;

pub struct OwnedTransactionIndexKey {
    block_height: BlockHeight,
    tx_idx: TransactionIndex,
}

impl From<Vec<u8>> for OwnedTransactionIndexKey {
    fn from(bytes: Vec<u8>) -> Self {
        // the first 32 bytes are the owner, which is already known when querying
        let mut block_height_bytes: [u8; 4] = Default::default();
        block_height_bytes.copy_from_slice(&bytes[32..36]);
        let mut tx_idx_bytes: [u8; 4] = Default::default();
        tx_idx_bytes.copy_from_slice(&bytes[36..40]);

        Self {
            // owner: Address::from(owner_bytes),
            block_height: u32::from_be_bytes(block_height_bytes).into(),
            tx_idx: u32::from_be_bytes(tx_idx_bytes),
        }
    }
}

#[derive(Debug, PartialOrd, Eq, PartialEq)]
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
        let mut tx_idx_bytes: [u8; 4] = Default::default();
        tx_idx_bytes.copy_from_slice(&bytes[4..8]);

        Self {
            block_height: u32::from_be_bytes(block_height_bytes).into(),
            tx_idx: u32::from_be_bytes(tx_idx_bytes),
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
