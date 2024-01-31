use serde::{
    Deserialize,
    Serialize,
};

use fuel_core_compression::ChangesPerTable;
use fuel_core_types::fuel_tx::CompactTransaction;

use crate::header::Header;

/// Compressed block.
/// The versioning here working depends on the serialization format,
/// but as long as we we have less than 128 variants, postcard will
/// make that a single byte.
#[derive(Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CompressedBlock {
    V0 {
        /// Registration section of the compressed block
        registrations: ChangesPerTable,
        /// Compressed block header
        header: Header,
        /// Compressed transactions
        transactions: Vec<CompactTransaction>,
    },
}

#[cfg(test)]
mod tests {
    use fuel_core_compression::{
        Compactable,
        CompactionContext,
        InMemoryRegistry,
    };
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        fuel_tx::Transaction,
        tai64::Tai64,
    };

    use super::*;

    #[test]
    fn postcard_roundtrip() {
        let original = CompressedBlock::V0 {
            registrations: ChangesPerTable::default(),
            header: Header {
                da_height: DaBlockHeight::default(),
                prev_root: Default::default(),
                height: 3u32.into(),
                time: Tai64::now(),
            },
            transactions: vec![],
        };

        let compressed = postcard::to_allocvec(&original).unwrap();
        let decompressed: CompressedBlock = postcard::from_bytes(&compressed).unwrap();

        let CompressedBlock::V0 {
            registrations,
            header,
            transactions,
        } = decompressed;

        assert_eq!(registrations, ChangesPerTable::default());
        assert_eq!(header.height, 3u32.into());
        assert!(transactions.is_empty());
    }

    #[test]
    fn compact_transactions() {
        let tx = Transaction::default_test_tx();

        let mut registry = InMemoryRegistry::default();
        let compacted = CompactionContext::run(&mut registry, tx.clone());

        let compressed = postcard::to_allocvec(&compacted).unwrap();
        dbg!(compressed.len());

        let decompacted = Transaction::decompact(compacted, &registry);

        assert_eq!(tx, decompacted);
    }
}
