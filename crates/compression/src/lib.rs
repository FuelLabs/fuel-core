pub mod db;
mod eviction_policy;
pub mod ports;
mod tables;
pub mod services {
    pub mod compress;
    pub mod decompress;
}
mod context {
    pub mod compress;
    pub mod decompress;
    pub mod prepare;
}

use serde::{
    Deserialize,
    Serialize,
};

use fuel_core_types::{
    blockchain::{
        header::{
            ConsensusParametersVersion,
            StateTransitionBytecodeVersion,
        },
        primitives::DaBlockHeight,
    },
    fuel_tx::CompressedTransaction,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    tai64::Tai64,
};
use tables::RegistrationsPerTable;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Header {
    pub da_height: DaBlockHeight,
    pub prev_root: Bytes32,
    pub height: BlockHeight,
    pub time: Tai64,
    pub consensus_parameters_version: ConsensusParametersVersion,
    pub state_transition_bytecode_version: StateTransitionBytecodeVersion,
}

/// Compressed block, without the preceding version byte.
#[derive(Clone, Serialize, Deserialize)]
struct CompressedBlockPayload {
    /// Registration section of the compressed block
    registrations: RegistrationsPerTable,
    /// Compressed block header
    header: Header,
    /// Compressed transactions
    transactions: Vec<CompressedTransaction>,
}

#[cfg(test)]
mod tests {
    use std::array;

    use db::RocksDb;
    use fuel_core_types::{
        blockchain::{
            block::{
                Block,
                PartialFuelBlock,
            },
            header::{
                ApplicationHeader,
                ConsensusHeader,
                PartialBlockHeader,
            },
            primitives::{
                DaBlockHeight,
                Empty,
            },
        },
        fuel_tx::Transaction,
        tai64::Tai64,
    };
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn postcard_roundtrip() {
        let original = CompressedBlockPayload {
            registrations: RegistrationsPerTable::default(),
            header: Header {
                da_height: DaBlockHeight::default(),
                prev_root: Default::default(),
                height: 3u32.into(),
                consensus_parameters_version: 1,
                state_transition_bytecode_version: 2,
                time: Tai64::UNIX_EPOCH,
            },
            transactions: vec![],
        };

        let compressed = postcard::to_allocvec(&original).unwrap();
        let decompressed: CompressedBlockPayload =
            postcard::from_bytes(&compressed).unwrap();

        let CompressedBlockPayload {
            registrations,
            header,
            transactions,
        } = decompressed;

        assert!(registrations.is_empty());
        assert_eq!(header.height, 3u32.into());
        assert!(transactions.is_empty());
    }

    #[test]
    fn same_compact_tx_is_smaller_in_next_block() {
        let tx = Transaction::default_test_tx();

        let tmpdir = TempDir::new().unwrap();
        let mut db = RocksDb::open(tmpdir.path()).unwrap();

        let sizes: [usize; 3] = array::from_fn(|h| {
            services::compress::compress(
                &mut db,
                Block::new(
                    PartialBlockHeader {
                        application: ApplicationHeader {
                            da_height: DaBlockHeight::default(),
                            consensus_parameters_version: 4,
                            state_transition_bytecode_version: 5,
                            generated: Empty,
                        },
                        consensus: ConsensusHeader {
                            prev_root: Bytes32::default(),
                            height: (h as u32).into(),
                            time: Tai64::UNIX_EPOCH,
                            generated: Empty,
                        },
                    },
                    vec![tx.clone()],
                    &[],
                    Bytes32::default(),
                )
                .expect("Invalid block header"),
            )
            .unwrap()
            .len()
        });

        assert!(sizes[0] > sizes[1], "Size must decrease after first block");
        assert!(
            sizes[1] == sizes[2],
            "Size must be constant after first block"
        );
    }

    #[test]
    fn compress_decompress_roundtrip() {
        let tx = Transaction::default_test_tx();

        let tmpdir = TempDir::new().unwrap();
        let mut db = RocksDb::open(tmpdir.path()).unwrap();

        let original_blocks: [Block; 3] = array::from_fn(|h| {
            Block::new(
                PartialBlockHeader {
                    application: ApplicationHeader {
                        da_height: DaBlockHeight::default(),
                        consensus_parameters_version: 4,
                        state_transition_bytecode_version: 5,
                        generated: Empty,
                    },
                    consensus: ConsensusHeader {
                        prev_root: Bytes32::default(),
                        height: (h as u32).into(),
                        time: Tai64::UNIX_EPOCH,
                        generated: Empty,
                    },
                },
                vec![tx.clone()],
                &[],
                Bytes32::default(),
            )
            .expect("Invalid block header")
        });

        let compressed_bytes: [Vec<u8>; 3] = original_blocks
            .clone()
            .map(|block| services::compress::compress(&mut db, block).unwrap());

        db.db.flush().unwrap();
        drop(tmpdir);
        let tmpdir2 = TempDir::new().unwrap();
        let mut db = RocksDb::open(tmpdir2.path()).unwrap();

        let decompressed_blocks: [PartialFuelBlock; 3] = array::from_fn(|h| {
            services::decompress::decompress(&mut db, compressed_bytes[h].clone())
                .expect("Decompression failed")
        });

        for (original, decompressed) in
            original_blocks.iter().zip(decompressed_blocks.iter())
        {
            assert_eq!(PartialFuelBlock::from(original.clone()), *decompressed);
        }
    }
}
