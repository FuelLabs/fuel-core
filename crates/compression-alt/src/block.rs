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
    use std::array;

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
    fn compact_transaction() {
        let tx = Transaction::default_test_tx();
        let mut registry = InMemoryRegistry::default();
        let compacted = CompactionContext::run(&mut registry, tx.clone());
        let decompacted = Transaction::decompact(compacted.clone(), &registry);
        assert_eq!(tx, decompacted);

        // Check size reduction
        let compressed_original = postcard::to_allocvec(&tx).unwrap();
        let compressed_compact = postcard::to_allocvec(&compacted).unwrap();
        assert!(compressed_compact.len() < compressed_original.len() / 2); // Arbitrary threshold
    }

    #[test]
    fn compact_transaction_twice_gives_equal_result() {
        let tx = Transaction::default_test_tx();
        let mut registry = InMemoryRegistry::default();
        let compacted1 = CompactionContext::run(&mut registry, tx.clone());
        let compacted2 = CompactionContext::run(&mut registry, tx.clone());
        let compressed1 = postcard::to_allocvec(&compacted1).unwrap();
        let compressed2 = postcard::to_allocvec(&compacted2).unwrap();
        assert_eq!(compressed1, compressed2);
    }

    #[test]
    fn sizes_of_repeated_tx_make_sense() {
        let tx = Transaction::default_test_tx();

        let sizes: [usize; 3] = array::from_fn(|i| {
            let mut registry = InMemoryRegistry::default();

            let original = CompressedBlock::V0 {
                registrations: ChangesPerTable::default(),
                header: Header {
                    da_height: DaBlockHeight::default(),
                    prev_root: Default::default(),
                    height: 3u32.into(),
                    time: Tai64::now(),
                },
                transactions: vec![
                    CompactionContext::run(&mut registry, tx.clone());
                    i + 1
                ],
            };

            let compressed = postcard::to_allocvec(&original).unwrap();
            compressed.len()
        });

        dbg!(sizes);
        panic!();

        assert!(sizes[0] < sizes[1]);
        assert!(sizes[1] < sizes[2]);
        let d1 = sizes[1] - sizes[0];
        let d2 = sizes[2] - sizes[1];
        assert!(d2 < d1);
    }
}
