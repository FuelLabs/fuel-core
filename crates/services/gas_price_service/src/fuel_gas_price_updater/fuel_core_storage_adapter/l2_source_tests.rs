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

#[derive(Debug, Clone, Default)]
struct FakeSerializer {
    bytes_count: u64,
}
impl FakeSerializer {
    fn new(bytes_count: u64) -> Self {
        Self { bytes_count }
    }
}

impl SerializeBytes for FakeSerializer {
    fn serialize_bytes<B: Serialize>(&self, _block: B) -> u64 {
        self.bytes_count
    }
}

fn l2_source<T, S>(
    gas_price_settings: T,
    serializer: S,
    committed_block_stream: BoxStream<SharedImportResult>,
) -> FuelL2BlockSource<T, S> {
    FuelL2BlockSource {
        gas_price_settings,
        serializer,
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

fn block_to_import_result(block: Block) -> Arc<ImportResult> {
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
    let (block, _mint) = build_block(&params.chain_id());
    let block_height = 1u32.into();
    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let serializer = FakeSerializer::default();
    let expected =
        get_block_info(&serializer, &block, gas_price_factor, block_gas_limit).unwrap();
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);
    let import_result = block_to_import_result(block.clone());
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> =
        vec![import_result];
    let block_stream = tokio_stream::iter(blocks).into_boxed();

    let mut source = l2_source(settings, serializer, block_stream);

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
    let (block, _mint) = build_block(&params.chain_id());

    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);
    let (_sender, receiver) = tokio::sync::mpsc::channel(1);
    let stream = ReceiverStream::new(receiver);
    let block_stream = Box::pin(stream);
    let serializer = FakeSerializer::default();
    let mut source = l2_source(settings, serializer.clone(), block_stream);

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
    _sender.send(import_result).await.unwrap();

    // then
    let actual = fut_l2_block.await.unwrap();
    let expected =
        get_block_info(&serializer, &block, gas_price_factor, block_gas_limit).unwrap();
    assert_eq!(expected, actual);
}

fn build_block(chain_id: &ChainId) -> (Block, Transaction) {
    let mut inner_mint = Mint::default();
    *inner_mint.gas_price_mut() = 500;
    *inner_mint.mint_amount_mut() = 1000;

    let tx = Transaction::Mint(inner_mint);
    let tx_id = tx.id(chain_id);
    let mut block = CompressedBlock::default();
    block.transactions_mut().push(tx_id);
    let new = block.uncompress(vec![tx.clone()]);
    (new, tx)
}

#[tokio::test]
async fn get_l2_block__calculates_fullness_correctly() {
    // given
    let chain_id = ChainId::default();
    let (block, mint) = build_block(&chain_id);
    let block_height = 1u32.into();

    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);
    let serializer = FakeSerializer::default();

    let import_result = block_to_import_result(block.clone());
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> =
        vec![import_result];
    let block_stream = tokio_stream::iter(blocks).into_boxed();

    let mut source = l2_source(settings, serializer, block_stream);

    // when
    let result = source.get_l2_block(block_height).await.unwrap();

    // then
    let actual = result.fullness;
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
async fn get_l2_block__calculates_block_bytes_correctly() {
    // given
    let chain_id = ChainId::default();
    let (block, _mint) = build_block(&chain_id);
    let block_height = 1u32.into();

    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);
    let expected_bytes = 1000;
    let serializer = FakeSerializer::new(expected_bytes);

    let import_result = block_to_import_result(block.clone());
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> =
        vec![import_result];
    let block_stream = tokio_stream::iter(blocks).into_boxed();

    let mut source = l2_source(settings, serializer, block_stream);

    // when
    let result = source.get_l2_block(block_height).await.unwrap();

    // then
    let actual = result.block_bytes;
    assert_eq!(expected_bytes, actual);
}

#[tokio::test]
async fn get_l2_block__retrieves_gas_price_correctly() {
    // given
    let chain_id = ChainId::default();
    let (block, mint) = build_block(&chain_id);
    let block_height = 1u32.into();

    let gas_price_factor = 100;
    let block_gas_limit = 1000;
    let settings = FakeSettings::new(gas_price_factor, block_gas_limit);
    let serializer = FakeSerializer::default();

    let import_result = block_to_import_result(block.clone());
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> =
        vec![import_result];
    let block_stream = tokio_stream::iter(blocks).into_boxed();

    let mut source = l2_source(settings, serializer, block_stream);

    // when
    let result = source.get_l2_block(block_height).await.unwrap();

    // then
    let actual = result.gas_price;
    let expected = mint.as_mint().unwrap().gas_price();
    assert_eq!(*expected, actual);
}
