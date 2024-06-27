#![allow(non_snake_case)]

use super::*;
use fuel_core::database::Database;
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::StorageAsMut;
use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_tx::{
        consensus_parameters::{
            ConsensusParametersV1,
            FeeParameters,
            FeeParametersV1,
        },
        ConsensusParameters,
        Mint,
        UniqueIdentifier,
    },
    fuel_types::ChainId,
};
use futures::future::{
    maybe_done,
    MaybeDone,
};
use std::time::Duration;

fn l2_source(
    database: Database,
    committed_block_stream: BoxStream<SharedImportResult>,
) -> FuelL2BlockSource<Database> {
    FuelL2BlockSource {
        database,
        committed_block_stream,
    }
}

fn params() -> ConsensusParameters {
    let mut params = ConsensusParametersV1::default();
    let mut fee_params = FeeParametersV1::default();
    fee_params.gas_price_factor = 100u64.into();
    params.fee_params = FeeParameters::V1(fee_params);
    ConsensusParameters::V1(params)
}

#[tokio::test]
async fn get_l2_block__gets_expected_value() {
    // given
    let block = CompressedBlock::default();
    let block_height = 1u32.into();
    let params = params();
    let block_info = get_block_info(&block.clone().uncompress(vec![]), &params);
    let mut database = Database::default();
    let version = block.header().consensus_parameters_version;
    database
        .storage_as_mut::<ConsensusParametersVersions>()
        .insert(&version, &params)
        .unwrap();
    database
        .storage_as_mut::<FuelBlocks>()
        .insert(&block_height, &block)
        .unwrap();
    let block_stream = Box::pin(tokio_stream::pending());

    let mut source = l2_source(database, block_stream);

    // when
    let result = source.get_l2_block(block_height).await.unwrap();

    // then
    assert_eq!(result, block_info);
}

#[tokio::test]
async fn get_l2_block__waits_for_block() {
    // given
    let block_height = 1u32.into();
    let block = CompressedBlock::default();
    let mut database = Database::default();
    let block_stream = Box::pin(tokio_stream::pending());
    let mut source = l2_source(database.clone(), block_stream);
    let params = params();
    let version = block.header().consensus_parameters_version;
    database
        .storage_as_mut::<ConsensusParametersVersions>()
        .insert(&version, &params)
        .unwrap();

    // when
    let mut fut_l2_block = source.get_l2_block(block_height);
    for _ in 0..10 {
        fut_l2_block = match maybe_done(fut_l2_block) {
            MaybeDone::Future(fut) => {
                const ARB_DURATION: u64 = 10;
                tokio::time::sleep(Duration::from_millis(ARB_DURATION)).await;
                fut
            }
            _ => panic!("Shouldn't be done yet"),
        };
    }
    database
        .storage_as_mut::<FuelBlocks>()
        .insert(&block_height, &block)
        .unwrap();

    // then
    let actual = fut_l2_block.await.unwrap();
    let uncompressed_block = block.uncompress(vec![]);
    let expected = get_block_info(&uncompressed_block, &params);
    assert_eq!(expected, actual);
}

fn build_block(chain_id: &ChainId) -> (CompressedBlock, Transaction) {
    let mut inner_mint = Mint::default();
    *inner_mint.gas_price_mut() = 500;
    *inner_mint.mint_amount_mut() = 1000;

    let tx = Transaction::Mint(inner_mint);
    let tx_id = tx.id(&chain_id);
    let mut block = CompressedBlock::default();
    block.transactions_mut().push(tx_id);
    (block, tx)
}

#[tokio::test]
async fn get_l2_block__calculates_fullness_correctly() {
    // given
    let params = params();
    let (block, mint) = build_block(&params.chain_id());
    let block_height = 1u32.into();
    let mut database = Database::default();
    let version = block.header().consensus_parameters_version;
    database
        .storage_as_mut::<ConsensusParametersVersions>()
        .insert(&version, &params)
        .unwrap();
    database
        .storage_as_mut::<Transactions>()
        .insert(&mint.id(&params.chain_id()), &mint)
        .unwrap();
    database
        .storage_as_mut::<FuelBlocks>()
        .insert(&block_height, &block)
        .unwrap();

    let block_stream = Box::pin(tokio_stream::pending());
    let mut source = l2_source(database, block_stream);

    // when
    let result = source.get_l2_block(block_height).await.unwrap();

    // then
    let actual = result.fullness;
    let block_gas_limit = params.block_gas_limit();
    let gas_price_factor = params.fee_params().gas_price_factor();
    let (fee, gas_price) = if let Transaction::Mint(inner_mint) = &mint {
        let fee = inner_mint.mint_amount();
        let gas_price = inner_mint.gas_price();
        (fee, gas_price)
    } else {
        panic!("Expected mint transaction")
    };

    let used = fee * gas_price_factor / gas_price;
    let expected = (used, block_gas_limit);
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn get_l2_block__calculates_block_bytes_correctly() {}

#[tokio::test]
async fn get_l2_block__retrieves_gas_price_correctly() {}
