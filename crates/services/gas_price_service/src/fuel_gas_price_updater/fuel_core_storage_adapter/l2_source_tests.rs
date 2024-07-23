#![allow(non_snake_case)]

use super::*;
use fuel_core_services::stream::{
    BoxStream,
    IntoBoxStream,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        SealedBlock,
    },
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
    services::block_importer::ImportResult,
};
use futures::future::{
    maybe_done,
    MaybeDone,
};
use std::{
    ops::Deref,
    sync::Arc,
    time::Duration,
};
use tokio_stream::wrappers::ReceiverStream;

struct FakeSettings {
    gas_price_factor: u64,
    block_gas_limit: u64,
}

impl FakeSettings {
    fn new(gas_price_factor: u64, block_gas_limit: u64) -> Self {
        Self {
            gas_price_factor,
            block_gas_limit,
        }
    }
}

impl GasPriceSettingsProvider for FakeSettings {
    fn settings(
        &self,
        _param_version: &ConsensusParametersVersion,
    ) -> Result<GasPriceSettings> {
        Ok(GasPriceSettings {
            gas_price_factor: self.gas_price_factor,
            block_gas_limit: self.block_gas_limit,
        })
    }
}

fn l2_source<T>(
    genesis_block_height: BlockHeight,
    gas_price_settings: T,
    committed_block_stream: BoxStream<SharedImportResult>,
) -> FuelL2BlockSource<T> {
    FuelL2BlockSource {
        genesis_block_height,
        gas_price_settings,
        committed_block_stream,
    }
}

fn params() -> ConsensusParameters {
    let mut params = ConsensusParametersV1::default();
    let fee_params = FeeParametersV1 {
        gas_price_factor: 100u64,
        ..Default::default()
    };
    params.fee_params = FeeParameters::V1(fee_params);
    ConsensusParameters::V1(params)
}

fn block_to_import_result(block: Block) -> SharedImportResult {
    let sealed_block = SealedBlock {
        entity: block,
        consensus: Default::default(),
    };
    let result = ImportResult {
        sealed_block,
        tx_status: vec![],
        events: vec![],
        source: Default::default(),
    };
    Arc::new(result)
}

#[tokio::test]
async fn get_l2_block__gets_expected_value() {
    // given
    let params = params();
    let height = 1u32.into();
    let (block, _mint) = build_block(&params.chain_id(), height);
    let block_height = 1u32.into();
    let genesis_block_height = 0u32.into();
    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let expected = get_block_info(&block, gas_price_factor, block_gas_limit).unwrap();
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);
    let import_result = block_to_import_result(block.clone());
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> =
        vec![import_result];
    let block_stream = tokio_stream::iter(blocks).into_boxed();

    let mut source = l2_source(genesis_block_height, settings, block_stream);

    // when
    let actual = source.get_l2_block(block_height).await.unwrap();

    // then
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn get_l2_block__waits_for_block() {
    // given
    let block_height = 1u32.into();
    let params = params();
    let (block, _mint) = build_block(&params.chain_id(), block_height);

    let genesis_block_height = 0u32.into();
    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let stream = ReceiverStream::new(receiver);
    let block_stream = Box::pin(stream);
    let mut source = l2_source(genesis_block_height, settings, block_stream);

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

    let import_result = block_to_import_result(block.clone());
    sender.send(import_result).await.unwrap();

    // then
    let actual = fut_l2_block.await.unwrap();
    let expected = get_block_info(&block, gas_price_factor, block_gas_limit).unwrap();
    assert_eq!(expected, actual);
}

fn build_block(chain_id: &ChainId, height: BlockHeight) -> (Block, Transaction) {
    let mut inner_mint = Mint::default();
    *inner_mint.gas_price_mut() = 500;
    *inner_mint.mint_amount_mut() = 1000;

    let tx = Transaction::Mint(inner_mint);
    let tx_id = tx.id(chain_id);
    let mut block = CompressedBlock::default();
    block.transactions_mut().push(tx_id);
    block.header_mut().consensus_mut().height = height;
    let new = block.uncompress(vec![tx.clone()]);
    (new, tx)
}

#[tokio::test]
async fn get_l2_block__calculates_gas_used_correctly() {
    // given
    let chain_id = ChainId::default();
    let block_height = 1u32.into();
    let (block, mint) = build_block(&chain_id, block_height);

    let genesis_block_height = 0u32.into();
    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);

    let import_result = block_to_import_result(block.clone());
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> =
        vec![import_result];
    let block_stream = tokio_stream::iter(blocks).into_boxed();

    let mut source = l2_source(genesis_block_height, settings, block_stream);

    // when
    let result = source.get_l2_block(block_height).await.unwrap();

    // then
    let BlockInfo::Block {
        gas_used: actual, ..
    } = result
    else {
        panic!("Expected non-genesis");
    };
    let (fee, gas_price) = if let Transaction::Mint(inner_mint) = &mint {
        let fee = inner_mint.mint_amount();
        let gas_price = inner_mint.gas_price();
        (fee, gas_price)
    } else {
        panic!("Expected mint transaction")
    };

    let used = fee * gas_price_factor / gas_price;
    let expected = used;
    assert_eq!(expected, actual);
}
#[tokio::test]
async fn get_l2_block__calculates_block_gas_capacity_correctly() {
    // given
    let chain_id = ChainId::default();
    let block_height = 1u32.into();
    let (block, _mint) = build_block(&chain_id, block_height);

    let genesis_block_height = 0u32.into();
    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);

    let import_result = block_to_import_result(block.clone());
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> =
        vec![import_result];
    let block_stream = tokio_stream::iter(blocks).into_boxed();

    let mut source = l2_source(genesis_block_height, settings, block_stream);

    // when
    let result = source.get_l2_block(block_height).await.unwrap();

    // then
    let BlockInfo::Block {
        block_gas_capacity: actual,
        ..
    } = result
    else {
        panic!("Expected non-genesis");
    };
    let expected = block_gas_limit;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn get_l2_block__if_block_precedes_genesis_block_throw_an_error() {
    // given
    let chain_id = ChainId::default();
    let block_height = 1u32.into();
    let (block, _mint) = build_block(&chain_id, block_height);

    let genesis_block_height = 2u32.into();
    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);

    let import_result = block_to_import_result(block.clone());
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> =
        vec![import_result];
    let block_stream = tokio_stream::iter(blocks).into_boxed();

    let mut source = l2_source(genesis_block_height, settings, block_stream);

    // when
    let error = source.get_l2_block(block_height).await.unwrap_err();

    // then
    assert!(matches!(error, GasPriceError::CouldNotFetchL2Block { .. }));
}
