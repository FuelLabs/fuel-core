use super::*;
use crate as fuel_core_poa;
use fuel_core_poa::ports::MockDatabase;
use fuel_core_types::{
    blockchain::header::{
        ApplicationHeader,
        ConsensusHeader,
        GeneratedApplicationFields,
        GeneratedConsensusFields,
        PartialBlockHeader,
    },
    fuel_tx::Transaction,
    tai64::Tai64,
};
use test_case::test_case;

struct Input {
    block_header_merkle_root: [u8; 32],
    prev_header_time: Tai64,
    prev_header_da_height: u64,
    ch: ConsensusHeader<GeneratedConsensusFields>,
    ah: ApplicationHeader<GeneratedApplicationFields>,
    txs: Vec<Transaction>,
}

fn correct() -> Input {
    let txs = vec![Transaction::default_test_tx()];
    let partial_header = PartialBlockHeader {
        application: ApplicationHeader {
            da_height: 2u64.into(),
            ..Default::default()
        },
        consensus: ConsensusHeader {
            prev_root: [2u8; 32].into(),
            height: 2u32.into(),
            time: Tai64(2),
            ..Default::default()
        },
    };
    let block_header = partial_header
        .generate(&txs, &[], Default::default())
        .unwrap();

    Input {
        block_header_merkle_root: [2u8; 32],
        prev_header_time: Tai64(2),
        prev_header_da_height: 2,
        ch: *block_header.consensus(),
        ah: *block_header.application(),
        txs,
    }
}

#[test_case(correct() => matches Ok(_) ; "genesis verify correct block")]
#[test_case(
    {
        let mut i = correct();
        i.ch.height = 0u32.into();
        i
    } => matches Err(_) ; "Height 0"
)]
#[test_case(
    {
        let mut i = correct();
        i.ch.prev_root = [3u8; 32].into();
        i
    } => matches Err(_) ; "genesis verify prev root mismatch should error"
)]
#[test_case(
    {
        let mut i = correct();
        i.ah.da_height = 1u64.into();
        i
    } => matches Err(_) ; "genesis verify da height lower then prev header should error"
)]
#[test_case(
    {
        let mut i = correct();
        i.ch.generated.application_hash = [0u8; 32].into();
        i
    } => matches Err(_) ; "genesis verify application hash mismatch should error"
)]
#[test_case(
    {
        let mut i = correct();
        i.ch.time = Tai64(1);
        i
    } => matches Err(_) ; "genesis verify time before prev header should error"
)]
#[test_case(
    {
        let mut i = correct();
        i.txs = vec![];
        i
    } => matches Err(_) ; "genesis verify wrong transactions"
)]
fn test_verify_genesis_block_fields(input: Input) -> anyhow::Result<()> {
    let Input {
        block_header_merkle_root,
        prev_header_time,
        prev_header_da_height,
        ch,
        ah,
        txs,
    } = input;
    let mut d = MockDatabase::default();
    d.expect_block_header_merkle_root()
        .returning(move |_| Ok(block_header_merkle_root.into()));
    d.expect_block_header().returning(move |_| {
        let mut h = BlockHeader::default();
        h.set_time(prev_header_time);
        h.set_da_height(prev_header_da_height.into());
        Ok(h)
    });
    let mut b = Block::default();
    b.header_mut().set_consensus_header(ch);
    b.header_mut().set_application_header(ah);
    *b.transactions_mut() = txs;
    verify_block_fields(&d, &b)
}
