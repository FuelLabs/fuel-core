use super::*;
use crate as fuel_core_poa;
use fuel_core_poa::ports::MockDatabase;
use fuel_core_types::{
    blockchain::header::{
        ApplicationHeader,
        ConsensusHeader,
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
    bh: BlockHeader,
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
        .generate(
            &txs,
            &[],
            Default::default(),
            #[cfg(feature = "fault-proving")]
            &Default::default(),
        )
        .unwrap();

    Input {
        block_header_merkle_root: [2u8; 32],
        prev_header_time: Tai64(2),
        prev_header_da_height: 2,
        bh: block_header,
        txs,
    }
}

#[test_case(correct() => matches Ok(_) ; "genesis verify correct block")]
#[test_case(
    {
        let mut i = correct();
        i.bh.set_block_height(0u32.into());
        i
    } => matches Err(_) ; "Height 0"
)]
#[test_case(
    {
        let mut i = correct();
        i.bh.set_previous_root([3u8; 32].into());
        i
    } => matches Err(_) ; "genesis verify prev root mismatch should error"
)]
#[test_case(
    {
        let mut i = correct();
        i.bh.set_da_height(1u64.into());
        i
    } => matches Err(_) ; "genesis verify da height lower then prev header should error"
)]
#[test_case(
    {
        let mut i = correct();
        i.bh.set_application_hash([0u8; 32].into());
        i
    } => matches Err(_) ; "genesis verify application hash mismatch should error"
)]
#[test_case(
    {
        let mut i = correct();
        i.bh.set_time(Tai64(1));
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
        bh,
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
    b.header_mut().set_consensus_header(*bh.consensus());
    match (bh, b.header_mut()) {
        (BlockHeader::V1(bh), BlockHeader::V1(h)) => {
            h.set_application_header(*bh.application())
        }
        #[cfg(feature = "fault-proving")]
        (BlockHeader::V2(bh), BlockHeader::V2(h)) => {
            h.set_application_header(*bh.application())
        }
        #[cfg_attr(not(feature = "fault-proving"), allow(unreachable_patterns))]
        _ => unreachable!(),
    }
    *b.transactions_mut() = txs;
    verify_block_fields(&d, &b)
}
