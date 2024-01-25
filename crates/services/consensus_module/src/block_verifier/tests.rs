use super::*;
use test_case::test_case;

#[test_case(
    {
        let mut h = BlockHeader::default();
        h.set_previous_root(Bytes32::zeroed());
        h.set_time(Tai64::UNIX_EPOCH);
        h.set_block_height(0u32.into());
        h
    },
    0 => matches Ok(_) ; "Correct header at `0`"
)]
#[test_case(
    {
        let mut h = BlockHeader::default();
        h.set_previous_root(Bytes32::zeroed());
        h.set_time(Tai64::UNIX_EPOCH);
        h.set_block_height(113u32.into());
        h
    },
    113 => matches Ok(_) ; "Correct header at `113`"
)]
#[test_case(
    {
        let mut h = BlockHeader::default();
        h.set_previous_root(Bytes32::zeroed());
        h.set_time(Tai64::UNIX_EPOCH);
        h.set_block_height(0u32.into());
        h
    },
    10 => matches Err(_) ; "wrong expected height"
)]
#[test_case(
    {
        let mut h = BlockHeader::default();
        h.set_previous_root(Bytes32::zeroed());
        h.set_time(Tai64::UNIX_EPOCH);
        h.set_block_height(5u32.into());
        h
    },
    0 => matches Err(_) ; "wrong header height"
)]
#[test_case(
    {
        let mut h = BlockHeader::default();
        h.set_previous_root(Bytes32::zeroed());
        h.set_time(Tai64(0));
        h.set_block_height(0u32.into());
        h
    },
    0 => matches Err(_) ; "wrong time"
)]
#[test_case(
    {
        let mut h = BlockHeader::default();
        h.set_previous_root(Bytes32::from([1u8; 32]));
        h.set_time(Tai64::UNIX_EPOCH);
        h.set_block_height(0u32.into());
        h
    },
    0 => matches Err(_) ; "wrong root"
)]
fn test_verify_genesis_block_fields(
    header: BlockHeader,
    expected_genesis_height: u32,
) -> anyhow::Result<()> {
    verify_genesis_block_fields(expected_genesis_height.into(), &header)
}
