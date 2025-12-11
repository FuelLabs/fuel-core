use super::*;
use crate::{
    block_range_response::RemoteS3Response,
    blocks::old_block_source::{
        BlockConvector,
        convertor_adapter::ConvertorAdapter,
    },
    db::table::{
        Column,
        Mode,
    },
};
use aws_sdk_s3::operation::put_object::PutObjectOutput;
use aws_smithy_mocks::{
    Rule,
    mock,
    mock_client,
};
use fuel_core_storage::{
    structured_storage::test::InMemoryStorage,
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
};
use fuel_core_types::blockchain::block::Block as FuelBlock;
use futures::StreamExt;

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

fn arb_proto_block() -> ProtoBlock {
    let block = FuelBlock::default();
    let convertor = ConvertorAdapter;
    convertor.convert_block(&block, &[]).unwrap()
}
fn put_happy_rule() -> Rule {
    mock!(Client::put_object)
        .match_requests(|req| req.bucket() == Some("test-bucket"))
        .sequence()
        .output(|| PutObjectOutput::builder().build())
        .build()
}

#[tokio::test]
async fn store_block__happy_path() {
    // given
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule()]);
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let mut adapter = RemoteCache::new(aws_bucket, client, storage, true);
    let block_height = BlockHeight::new(123);
    let block = arb_proto_block();

    // when
    let res = adapter.store_block(block_height, &block).await;

    // then
    assert!(res.is_ok());
}

#[tokio::test]
async fn get_block_range__happy_path() {
    // given
    let start = BlockHeight::new(999);
    let end = BlockHeight::new(1003);
    let aws_bucket = "test-bucket".to_string();
    let mut storage = database();
    storage
        .storage_as_mut::<LatestBlock>()
        .insert(&(), &Mode::new_s3(end))
        .unwrap();

    let adapter = RemoteBlocksProvider::new(aws_bucket.clone(), false, None, storage);

    // when
    let addresses = adapter.get_block_range(start, end).unwrap();

    // then
    let actual = match addresses {
        BlockRangeResponse::Literal(_) => {
            panic!("Expected remote response, got literal");
        }
        BlockRangeResponse::S3(stream) => stream.collect::<Vec<_>>().await,
    };
    let expected = (999..=1003)
        .map(|height| {
            let key = block_height_to_key(&BlockHeight::new(height));
            let res = RemoteS3Response {
                bucket: aws_bucket.clone(),
                key,
                requester_pays: false,
                aws_endpoint: None,
            };
            (BlockHeight::new(height), res)
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn get_current_height__returns_highest_continuous_block() {
    // given
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule()]);
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let mut adapter = RemoteCache::new(aws_bucket, client, storage, true);

    let expected = BlockHeight::new(123);
    let block = arb_proto_block();
    adapter.store_block(expected, &block).await.unwrap();

    // when
    let actual = adapter.get_current_height().unwrap().unwrap();

    // then
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn store_block__fails_if_not_contiguous() {
    let mut storage = database();

    // Given
    let mut tx = storage.write_transaction();
    let starting_height = BlockHeight::from(1u32);
    tx.storage_as_mut::<LatestBlock>()
        .insert(&(), &Mode::new_s3(starting_height))
        .unwrap();
    tx.commit().unwrap();
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule()]);
    let aws_bucket = "test-bucket".to_string();
    let mut adapter = RemoteCache::new(aws_bucket, client, storage, true);

    let expected = BlockHeight::new(3);
    let block = arb_proto_block();

    // When
    let result = adapter.store_block(expected, &block).await;

    // Then
    result.expect_err("expected error");
}
