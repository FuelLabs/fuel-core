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
        Compact,
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

    #[test]
    #[ignore = "This test is slow"]
    fn compact_registry_key_wraparound() {
        use fuel_core_types::fuel_types::AssetId;

        #[derive(Debug, Clone, Copy, PartialEq, Compact)]
        struct Example {
            #[da_compress(registry = "AssetId")]
            a: AssetId,
        }

        let mut registry = InMemoryRegistry::default();
        for i in 0u32..((1 << 24) + 100) {
            if i % 10000 == 0 {
                println!("i = {} ({})", i, (i as f32) / (1 << 24) as f32);
            }
            let mut bytes = [0x00; 32];
            bytes[..4].copy_from_slice(&i.to_be_bytes());
            let target = Example {
                a: AssetId::from(bytes),
            };
            let (compact, _) = CompactionContext::run(&mut registry, target);
            if i % 99 == 0 {
                let decompact = Example::decompact(compact, &registry);
                assert_eq!(decompact, target);
            }
        }
    }
}
