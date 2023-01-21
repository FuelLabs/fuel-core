use super::*;
use fuel_core_types::{
    blockchain::header::{
        ApplicationHeader,
        ConsensusHeader,
        GeneratedApplicationFields,
        GeneratedConsensusFields,
    },
    tai64::Tai64,
};
use test_case::test_case;

struct Input {
    c: Config,
    block_header_merkle_root: [u8; 32],
    prev_header_time: Tai64,
    prev_header_da_height: u64,
    ch: ConsensusHeader<GeneratedConsensusFields>,
    ah: ApplicationHeader<GeneratedApplicationFields>,
}

fn app_hash(da_height: u64) -> Bytes32 {
    ApplicationHeader {
        da_height: da_height.into(),
        ..Default::default()
    }
    .hash()
}

fn correct() -> Input {
    Input {
        c: Config {
            enabled_manual_blocks: false,
        },
        block_header_merkle_root: [2u8; 32],
        prev_header_time: Tai64(2),
        prev_header_da_height: 2,
        ch: ConsensusHeader {
            prev_root: [2u8; 32].into(),
            height: 2u32.into(),
            time: Tai64(2),
            generated: GeneratedConsensusFields {
                application_hash: app_hash(2),
            },
        },
        ah: ApplicationHeader {
            da_height: 2u64.into(),
            ..Default::default()
        },
    }
}

#[test_case(correct() => matches Ok(_) ; "Correct block")]
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
    } => matches Err(_) ; "Prev root mis-match"
)]
#[test_case(
    {
        let mut i = correct();
        i.ah.da_height = 1u64.into();
        i
    } => matches Err(_) ; "da height lower then prev header"
)]
#[test_case(
    {
        let mut i = correct();
        i.ch.generated.application_hash = [0u8; 32].into();
        i
    } => matches Err(_) ; "application hash mis-match"
)]
#[test_case(
    {
        let mut i = correct();
        i.ch.time = Tai64(1);
        i
    } => matches Err(_) ; "time before prev header"
)]
fn test_verify_genesis_block_fields(input: Input) -> anyhow::Result<()> {
    let Input {
        c,
        block_header_merkle_root,
        prev_header_time,
        prev_header_da_height,
        ch,
        ah,
    } = input;
    let mut d = MockDatabase::default();
    d.expect_block_header_merkle_root()
        .returning(move |_| Ok(block_header_merkle_root.into()));
    d.expect_block_header().returning(move |_| {
        let mut h = BlockHeader::default();
        h.consensus.time = prev_header_time;
        h.application.da_height = prev_header_da_height.into();
        Ok(h)
    });
    let mut b = Block::default();
    b.header_mut().consensus = ch;
    b.header_mut().application = ah;
    verify_poa_block_fields(&c, &d, &b)
}
