use serde::{
    Deserialize,
    Serialize,
};

use fuel_core_types::{
    fuel_compression::ChangesPerTable,
    fuel_tx::CompactTransaction,
};

use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    tai64::Tai64,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub da_height: DaBlockHeight,
    pub prev_root: Bytes32,
    pub height: BlockHeight,
    pub time: Tai64,
}

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

    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        fuel_compression::{
            Compact,
            Compactable,
            CompactionContext,
            InMemoryRegistry,
        },
        fuel_tx::Transaction,
        tai64::Tai64,
    };

    use super::*;

    #[test]
    fn postcard_roundtrip() {
        let original = CompressedBlock::V0 {
            registrations: ChangesPerTable::from_start_keys(Default::default()),
            header: Header {
                da_height: DaBlockHeight::default(),
                prev_root: Default::default(),
                height: 3u32.into(),
                time: Tai64::UNIX_EPOCH,
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

        assert!(registrations.is_empty());
        assert_eq!(header.height, 3u32.into());
        assert!(transactions.is_empty());
    }

    #[test]
    fn compact_transaction() {
        let tx = Transaction::default_test_tx();
        let mut registry = InMemoryRegistry::default();
        let (compacted, _) = CompactionContext::run(&mut registry, tx.clone());
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
        let (compacted1, changes1) = CompactionContext::run(&mut registry, tx.clone());
        let (compacted2, changes2) = CompactionContext::run(&mut registry, tx.clone());
        assert!(!changes1.is_empty());
        assert!(changes2.is_empty());
        let compressed1 = postcard::to_allocvec(&compacted1).unwrap();
        let compressed2 = postcard::to_allocvec(&compacted2).unwrap();
        assert_eq!(compressed1, compressed2);
    }

    #[test]
    fn sizes_of_repeated_tx_make_sense() {
        let tx = Transaction::default_test_tx();

        let sizes: [usize; 4] = array::from_fn(|i| {
            // Registry recreated for each block in this test
            let mut registry = InMemoryRegistry::default();

            let (transactions, registrations) =
                CompactionContext::run(&mut registry, vec![tx.clone(); i]);

            let original = CompressedBlock::V0 {
                registrations,
                header: Header {
                    da_height: DaBlockHeight::default(),
                    prev_root: Default::default(),
                    height: 3u32.into(),
                    time: Tai64::UNIX_EPOCH,
                },
                transactions,
            };

            let compressed = postcard::to_allocvec(&original).unwrap();
            compressed.len()
        });

        assert!(
            sizes.windows(2).all(|w| w[0] < w[1]),
            "Sizes should be in strictly ascending order"
        );
        let deltas: Vec<_> = sizes.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(deltas[0] > deltas[1], "Initial delta should be larger");
        assert!(deltas[1] == deltas[2], "Later delta should be constant");
    }

    #[test]
    fn same_compact_tx_is_smaller_in_next_block() {
        let tx = Transaction::default_test_tx();

        let mut registry = InMemoryRegistry::default();

        let sizes: [usize; 3] = array::from_fn(|_| {
            let (transactions, registrations) =
                CompactionContext::run(&mut registry, vec![tx.clone()]);

            let original = CompressedBlock::V0 {
                registrations,
                header: Header {
                    da_height: DaBlockHeight::default(),
                    prev_root: Default::default(),
                    height: 3u32.into(),
                    time: Tai64::UNIX_EPOCH,
                },
                transactions,
            };

            let compressed = postcard::to_allocvec(&original).unwrap();
            compressed.len()
        });

        assert!(sizes[0] > sizes[1], "Size must decrease after first block");
        assert!(
            sizes[1] == sizes[2],
            "Size must be constant after first block"
        );
    }
}
