use super::*;
use fuel_core_storage::codec::postcard::Postcard;
use fuel_core_types::fuel_tx::Mint;

fn block_with_transactions(transactions: Vec<Transaction>) -> Block {
    let mut block = Block::default();
    *block.transactions_mut() = transactions;
    block.header_mut().consensus_mut().height = 128u32.into();
    block
}

fn mint(fee: u64, gas_price: u64) -> Transaction {
    let mut mint = Mint::default();
    *mint.mint_amount_mut() = fee;
    *mint.gas_price_mut() = gas_price;
    Transaction::Mint(mint)
}

fn script(payload_size: usize) -> Transaction {
    Transaction::Script(Transaction::script(
        1_000,
        vec![0x24, 0, 0, 0],
        vec![0xab; payload_size],
        Default::default(),
        vec![],
        vec![],
        vec![],
    ))
}

#[test]
fn block_size_preserves_full_block_serialization() {
    let blocks = [
        block_with_transactions(vec![]),
        block_with_transactions(vec![mint(1_000, 500)]),
        block_with_transactions(vec![script(127), mint(1_000, 500)]),
        block_with_transactions(vec![script(128), script(1_024), mint(1_000, 500)]),
    ];
    for block in blocks {
        let expected = Postcard::encode(&block);
        assert_eq!(BlockCodec::encode(&block).as_bytes(), expected);
        assert_eq!(block_bytes(&block), expected.len() as u64);
    }
}

#[test]
fn block_info_preserves_gas_price_inputs() {
    let block = block_with_transactions(vec![script(1_024), mint(1_000, 500)]);
    let expected = BlockInfo::Block {
        height: 128,
        gas_used: 200,
        block_gas_capacity: 1_000,
        block_bytes: Postcard::encode(&block).len() as u64,
        block_fees: 1_000,
        gas_price: 500,
    };
    assert_eq!(get_block_info(&block, 100, 1_000).unwrap(), expected);
}

#[test]
fn block_info_preserves_missing_mint_and_overflow_errors() {
    for (block, message) in [
        (
            block_with_transactions(vec![script(128)]),
            "Block has no mint transaction",
        ),
        (
            block_with_transactions(vec![mint(u64::MAX, 1)]),
            "Failed to scale fee by gas price factor, overflow",
        ),
    ] {
        let error = get_block_info(&block, 100, 1_000).unwrap_err();
        let GasPriceError::CouldNotFetchL2Block { source_error } = error else {
            panic!("Unexpected error: {error:?}");
        };
        assert_eq!(source_error.to_string(), message);
    }
}
