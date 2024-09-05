use std::sync::{
    Arc,
    Mutex,
};

use bimap::BiMap;
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
    fuel_tx::{
        Bytes32,
        Transaction,
        TxPointer,
        UtxoId,
    },
    tai64::Tai64,
};
use tempfile::TempDir;

use crate::{
    db::RocksDb,
    ports::{
        TxPointerToUtxoId,
        UtxoIdToPointer,
    },
    services,
};

/// Just stores the looked-up tx pointers in a map, instead of actually looking them up.
#[derive(Default)]
pub struct MockTxDb {
    mapping: Arc<Mutex<BiMap<UtxoId, (TxPointer, u16)>>>,
}

#[async_trait::async_trait]
impl UtxoIdToPointer for MockTxDb {
    async fn lookup(&self, utxo_id: UtxoId) -> anyhow::Result<(TxPointer, u16)> {
        let mut g = self.mapping.lock().unwrap();
        if !g.contains_left(&utxo_id) {
            let key = g.len() as u32; // Just obtain an unique key
            g.insert(utxo_id, (TxPointer::new(key.into(), 0), 0));
        }
        Ok(g.get_by_left(&utxo_id).cloned().unwrap())
    }
}

#[async_trait::async_trait]
impl TxPointerToUtxoId for MockTxDb {
    async fn lookup(&self, tx_pointer: TxPointer, index: u16) -> anyhow::Result<UtxoId> {
        let g = self.mapping.lock().unwrap();
        g.get_by_right(&(tx_pointer, index))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "(TxPointer, index) not found in mock db: {:?}",
                    (tx_pointer, index)
                )
            })
    }
}

#[tokio::test]
async fn same_compact_tx_is_smaller_in_next_block() {
    let tx = Transaction::default_test_tx();

    let tmpdir = TempDir::new().unwrap();

    let mut db = RocksDb::open(tmpdir.path()).unwrap();
    let tx_db = MockTxDb::default();

    let mut sizes = Vec::new();
    for i in 0..3 {
        let compressed = services::compress::compress(
            &mut db,
            &tx_db,
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
                        height: i.into(),
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
        .await
        .unwrap();
        sizes.push(compressed.len());
    }

    assert!(sizes[0] > sizes[1], "Size must decrease after first block");
    assert!(
        sizes[1] == sizes[2],
        "Size must be constant after first block"
    );
}

#[tokio::test]
async fn compress_decompress_roundtrip() {
    let tx = Transaction::default_test_tx();

    let tmpdir = TempDir::new().unwrap();
    let mut db = RocksDb::open(tmpdir.path()).unwrap();
    let tx_db = MockTxDb::default();

    let mut original_blocks = Vec::new();
    let mut compressed_blocks = Vec::new();

    for i in 0..3 {
        let block = Block::new(
            PartialBlockHeader {
                application: ApplicationHeader {
                    da_height: DaBlockHeight::default(),
                    consensus_parameters_version: 4,
                    state_transition_bytecode_version: 5,
                    generated: Empty,
                },
                consensus: ConsensusHeader {
                    prev_root: Bytes32::default(),
                    height: i.into(),
                    time: Tai64::UNIX_EPOCH,
                    generated: Empty,
                },
            },
            vec![tx.clone()],
            &[],
            Bytes32::default(),
        )
        .expect("Invalid block header");
        original_blocks.push(block.clone());
        compressed_blocks.push(
            services::compress::compress(&mut db, &tx_db, block)
                .await
                .expect("Failed to compress"),
        );
    }

    db.db.flush().unwrap();
    drop(tmpdir);
    let tmpdir2 = TempDir::new().unwrap();
    let mut db = RocksDb::open(tmpdir2.path()).unwrap();

    for (original, compressed) in original_blocks
        .into_iter()
        .zip(compressed_blocks.into_iter())
    {
        let decompressed = services::decompress::decompress(&mut db, &tx_db, compressed)
            .await
            .expect("Decompression failed");
        assert_eq!(PartialFuelBlock::from(original), decompressed);
    }
}
