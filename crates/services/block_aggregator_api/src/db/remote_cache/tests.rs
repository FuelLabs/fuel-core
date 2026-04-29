use super::*;
use crate::{
    block_range_response::{
        RemoteBlockPayload,
        RemoteS3Response,
    },
    blocks::old_block_source::{
        BlockConverter,
        convertor_adapter::ProtobufBlockConverter,
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
use std::sync::Arc;

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

fn arb_proto_block_bytes() -> Arc<[u8]> {
    let block = FuelBlock::default();
    let convertor = ProtobufBlockConverter;
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
    let block = arb_proto_block_bytes();

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

    let adapter =
        RemoteBlocksProvider::new(aws_bucket.clone(), false, None, None, storage);

    // when
    let addresses = adapter.get_block_range(start, end).unwrap();

    // then
    let actual = match addresses {
        BlockRangeResponse::Remote(stream) => stream.collect::<Vec<_>>().await,
        _ => {
            panic!("Expected remote response, got literal");
        }
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
            (BlockHeight::new(height), RemoteBlockPayload::S3(res))
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn public_http_object_url__joins_base_and_key() {
    assert_eq!(
        public_http_object_url("https://cdn.example/blocks", "aa/bb/cc/dd"),
        "https://cdn.example/blocks/aa/bb/cc/dd"
    );
    assert_eq!(
        public_http_object_url("https://cdn.example/blocks/", "/aa/bb/cc/dd"),
        "https://cdn.example/blocks/aa/bb/cc/dd"
    );
}

#[tokio::test]
async fn get_current_height__returns_highest_continuous_block() {
    // given
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule()]);
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let mut adapter = RemoteCache::new(aws_bucket, client, storage, true);

    let expected = BlockHeight::new(123);
    let block = arb_proto_block_bytes();
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
    let block = arb_proto_block_bytes();

    // When
    let result = adapter.store_block(expected, &block).await;

    // Then
    result.expect_err("expected error");
}

/// When `PublicHttpConfig` is set on the provider, the range response must be HTTP payloads built
/// from `{base}/{s3-key}` URLs with the configured headers — not S3 bucket metadata. Clients use
/// this to fetch block objects via a CDN/public URL while uploads still go to S3.
#[tokio::test]
async fn get_block_range__with_public_http_config__returns_http_payloads() {
    // given
    use crate::{
        block_range_response::RemoteHttpResponse,
        db::remote_cache::PublicHttpConfig,
    };
    use std::collections::HashMap;

    let start = BlockHeight::new(10);
    let end = BlockHeight::new(12);
    let mut storage = database();
    storage
        .storage_as_mut::<LatestBlock>()
        .insert(&(), &Mode::new_s3(end))
        .unwrap();

    let headers: HashMap<String, String> =
        [("x-api-key".to_string(), "token".to_string())]
            .into_iter()
            .collect();
    let public_http = PublicHttpConfig {
        base_url: "https://cdn.example/blocks/".to_string(),
        headers: headers.clone(),
    };

    let adapter = RemoteBlocksProvider::new(
        "test-bucket".to_string(),
        false,
        None,
        Some(public_http),
        storage,
    );

    // when
    let response = adapter.get_block_range(start, end).unwrap();
    let actual = match response {
        BlockRangeResponse::Remote(stream) => stream.collect::<Vec<_>>().await,
        _ => panic!("expected remote response"),
    };

    // then
    let expected: Vec<_> = (10..=12)
        .map(|height| {
            let key = block_height_to_key(&BlockHeight::new(height));
            (
                BlockHeight::new(height),
                RemoteBlockPayload::Http(RemoteHttpResponse {
                    url: format!("https://cdn.example/blocks/{key}"),
                    headers: headers.clone(),
                }),
            )
        })
        .collect();
    assert_eq!(actual, expected);
}

/// Requesting a range that extends past the last synced block is the contract the gRPC server
/// relies on to surface "not yet available" to clients — the remote provider should refuse rather
/// than hand out URLs for nonexistent objects.
#[tokio::test]
async fn get_block_range__past_latest_synced__returns_error() {
    let mut storage = database();
    let synced = BlockHeight::new(5);
    storage
        .storage_as_mut::<LatestBlock>()
        .insert(&(), &Mode::new_s3(synced))
        .unwrap();

    let adapter =
        RemoteBlocksProvider::new("test-bucket".to_string(), false, None, None, storage);

    let result = adapter.get_block_range(BlockHeight::new(1), BlockHeight::new(6));
    result.expect_err("expected error for range past synced height");
}
