#![allow(non_snake_case)]

use aws_config::{
    BehaviorVersion,
    default_provider::credentials::DefaultCredentialsChain,
};
use aws_sdk_s3::Client;
use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_block_aggregator_api::{
    db::remote_cache::block_height_to_key,
    service::StorageMethod,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::*,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use rand::{
    SeedableRng,
    rngs::StdRng,
};
use std::iter;
use test_helpers::produce_block_with_tx;
use tokio::time::sleep;

const AWS_ENDPOINT_URL: &str = "http://127.0.0.1:4566";

macro_rules! require_env_var_or_panic {
    ($($var:literal),+) => {
        $(if std::env::var($var).is_err() {
            panic!("missing env var: {}", $var);
        })+
    };
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_height__can_get_value_from_rpc() {
    require_env_var_or_panic!("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION");

    // setup
    ensure_bucket_exists().await;
    clean_s3_bucket().await;
    let endpoint_url = AWS_ENDPOINT_URL.to_string();
    let storage_method = StorageMethod::S3 {
        bucket: "test-bucket".to_string(),
        endpoint_url: Some(endpoint_url),
        requester_pays: false,
        public_block_http_url: None,
        public_block_http_headers: Default::default(),
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);

    // given
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let fuel_client = FuelClient::new_with_rpc(
        iter::once(srv.bound_address.to_string()),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();

    let tx = Transaction::default_test_tx();
    let _ = fuel_client.submit_and_await_commit(&tx).await.unwrap();
    let expected_height = BlockHeight::new(1);

    // when
    sleep(std::time::Duration::from_secs(1)).await;
    let actual_height = fuel_client.get_aggregated_height().await.unwrap();

    // then
    assert_eq!(expected_height, actual_height);

    // cleanup
    clean_s3_bucket().await;
}

async fn aws_client() -> Client {
    let credentials = DefaultCredentialsChain::builder().build().await;
    let _aws_region =
        std::env::var("AWS_REGION").expect("AWS_REGION env var must be set");
    let sdk_config = aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(credentials)
        .endpoint_url(AWS_ENDPOINT_URL)
        .load()
        .await;
    let builder = aws_sdk_s3::config::Builder::from(&sdk_config);
    let config = builder.force_path_style(true).build();
    Client::from_conf(config)
}

async fn block_found_in_s3_bucket() -> bool {
    let client = aws_client().await;
    let bucket = "test-bucket".to_string();
    let key = block_height_to_key(&BlockHeight::new(1));
    tracing::info!(
        "checking if block is in bucket: {} with key {}",
        bucket,
        key
    );
    let req = client.get_object().bucket(&bucket).key(&key);
    req.send().await.is_ok()
}

async fn ensure_bucket_exists() {
    let client = aws_client().await;
    let bucket = "test-bucket";
    let req = client.create_bucket().bucket(bucket);
    let expect_message = format!("should be able to create bucket: {}", bucket);
    let _ = req.send().await.expect(&expect_message);
}

async fn clean_s3_bucket() {
    let client = aws_client().await;
    let bucket = "test-bucket";
    let req = client.list_objects().bucket(bucket);
    let objs = req.send().await.unwrap();
    for obj in objs.contents.unwrap_or_default() {
        let req = client.delete_object().bucket(bucket).key(obj.key.unwrap());
        let _ = req.send().await.unwrap();
    }
}

/// Allow unauthenticated GETs on the bucket so the client's reqwest-based HTTP path can read
/// objects directly (mirroring a real CDN-in-front-of-S3 deployment).
async fn allow_public_reads(bucket: &str) {
    let client = aws_client().await;
    let policy = format!(
        r#"{{"Version":"2012-10-17","Statement":[{{"Sid":"AllowPublicRead","Effect":"Allow","Principal":"*","Action":"s3:GetObject","Resource":"arn:aws:s3:::{bucket}/*"}}]}}"#
    );
    let _ = client
        .put_bucket_policy()
        .bucket(bucket)
        .policy(policy)
        .send()
        .await;
}

/// Waits for the block aggregator to catch up to `expected` or panics after a bounded retry window.
/// The aggregator runs in a background task so there is a small delay between block commit and
/// the S3 upload being visible to RPC consumers.
async fn wait_for_aggregated_height(client: &FuelClient, expected: BlockHeight) {
    for _ in 0..50 {
        if let Ok(height) = client.get_aggregated_height().await
            && height >= expected
        {
            return;
        }
        sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("block aggregator did not reach height {expected}");
}

fn public_http_base() -> String {
    format!("{AWS_ENDPOINT_URL}/test-bucket")
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__can_get_from_remote_s3_bucket() {
    // setup
    require_env_var_or_panic!("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION");
    ensure_bucket_exists().await;
    clean_s3_bucket().await;

    // given
    let endpoint_url = AWS_ENDPOINT_URL.to_string();
    let storage_method = StorageMethod::S3 {
        bucket: "test-bucket".to_string(),
        endpoint_url: Some(endpoint_url),
        requester_pays: false,
        public_block_http_url: None,
        public_block_http_headers: Default::default(),
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();
    let fuel_client = FuelClient::new_with_rpc(
        iter::once(srv.bound_address.to_string()),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();
    let tx = Transaction::default_test_tx();

    // when
    let _ = fuel_client.submit_and_await_commit(&tx).await.unwrap();

    sleep(std::time::Duration::from_secs(1)).await;

    // then
    let stream = fuel_client
        .get_block_range(BlockHeight::new(1), BlockHeight::new(1))
        .await
        .unwrap();
    futures::pin_mut!(stream);
    let (_, receipts) = stream.next().await.unwrap().unwrap();

    assert!(
        matches!(
            receipts[0][1],
            Receipt::ScriptResult {
                result: ScriptExecutionResult::Success,
                ..
            }
        ),
        "should have a script result receipt, received: {:?}",
        receipts
    );
    assert!(
        matches!(receipts[0][0], Receipt::Return { .. }),
        "should have a return receipt, received: {:?}",
        receipts
    );

    // cleanup
    clean_s3_bucket().await;
    drop(srv);
    tracing::info!(
        "Successfully ran test: get_block_range__can_get_from_remote_s3_bucket"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_height__no_publish__can_get_value_from_rpc() {
    require_env_var_or_panic!("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION");

    // setup
    ensure_bucket_exists().await;
    clean_s3_bucket().await;
    let endpoint_url = AWS_ENDPOINT_URL.to_string();
    let storage_method = StorageMethod::S3NoPublish {
        bucket: "test-bucket".to_string(),
        endpoint_url: Some(endpoint_url),
        requester_pays: false,
        public_block_http_url: None,
        public_block_http_headers: Default::default(),
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);

    // given
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let fuel_client = FuelClient::new_with_rpc(
        iter::once(srv.bound_address.to_string()),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();

    let tx = Transaction::default_test_tx();
    let _ = fuel_client.submit_and_await_commit(&tx).await.unwrap();
    let expected_height = BlockHeight::new(1);
    sleep(std::time::Duration::from_secs(1)).await;

    // when
    let actual_height = fuel_client.get_aggregated_height().await.unwrap();

    // then
    assert_eq!(expected_height, actual_height);

    // cleanup
    clean_s3_bucket().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_and_await_commit__no_publish__does_not_publish_to_s3_bucket() {
    // setup
    require_env_var_or_panic!("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION");
    ensure_bucket_exists().await;
    clean_s3_bucket().await;

    // given
    let endpoint_url = AWS_ENDPOINT_URL.to_string();
    let storage_method = StorageMethod::S3NoPublish {
        bucket: "test-bucket".to_string(),
        endpoint_url: Some(endpoint_url),
        requester_pays: false,
        public_block_http_url: None,
        public_block_http_headers: Default::default(),
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();
    let graphql_client = FuelClient::from(srv.bound_address);
    let tx = Transaction::default_test_tx();

    // when
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    sleep(std::time::Duration::from_secs(1)).await;

    // then
    let found_block = block_found_in_s3_bucket().await;
    assert!(!found_block);
}

/// Verifies the end-to-end HTTP endpoint path: the gRPC server returns `RemoteHttpEndpoint` URLs
/// that point at the (unsigned) S3 HTTP URL, and the client fetches + gzip-decompresses the block
/// object. This is the same plumbing a production deployment uses when placing a CDN in front of
/// the S3 bucket.
#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__public_http_endpoint__fetches_and_decompresses_block() {
    // setup
    require_env_var_or_panic!("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION");
    ensure_bucket_exists().await;
    clean_s3_bucket().await;
    allow_public_reads("test-bucket").await;

    // given
    let storage_method = StorageMethod::S3 {
        bucket: "test-bucket".to_string(),
        endpoint_url: Some(AWS_ENDPOINT_URL.to_string()),
        requester_pays: false,
        public_block_http_url: Some(public_http_base()),
        public_block_http_headers: Default::default(),
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();
    let fuel_client = FuelClient::new_with_rpc(
        iter::once(srv.bound_address.to_string()),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();
    let tx = Transaction::default_test_tx();
    let _ = fuel_client.submit_and_await_commit(&tx).await.unwrap();
    wait_for_aggregated_height(&fuel_client, BlockHeight::new(1)).await;

    // when
    let stream = fuel_client
        .get_block_range(BlockHeight::new(1), BlockHeight::new(1))
        .await
        .unwrap();
    futures::pin_mut!(stream);
    let (block, receipts) = stream.next().await.unwrap().unwrap();

    // then - the HTTP body came back gzipped (stored with content_encoding=gzip) and the client
    // decompressed it before decoding the protobuf block + receipts.
    assert_eq!(*block.header().height(), BlockHeight::new(1));
    assert!(
        matches!(
            receipts[0][1],
            Receipt::ScriptResult {
                result: ScriptExecutionResult::Success,
                ..
            }
        ),
        "should have a script result receipt, received: {:?}",
        receipts
    );
    assert!(
        matches!(receipts[0][0], Receipt::Return { .. }),
        "should have a return receipt, received: {:?}",
        receipts
    );

    clean_s3_bucket().await;
    drop(srv);
}

/// Requesting a range that spans multiple blocks over the public HTTP endpoint should stream each
/// block as a separate HTTP fetch and decompress each one.
#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__public_http_endpoint__streams_multi_block_range() {
    // setup
    require_env_var_or_panic!("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION");
    ensure_bucket_exists().await;
    clean_s3_bucket().await;
    allow_public_reads("test-bucket").await;

    // given
    const NUM_BLOCKS: u32 = 3;
    let storage_method = StorageMethod::S3 {
        bucket: "test-bucket".to_string(),
        endpoint_url: Some(AWS_ENDPOINT_URL.to_string()),
        requester_pays: false,
        public_block_http_url: Some(public_http_base()),
        public_block_http_headers: Default::default(),
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();
    let fuel_client = FuelClient::new_with_rpc(
        iter::once(srv.bound_address.to_string()),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();

    let mut rng = StdRng::seed_from_u64(0xC0FFEE);
    for _ in 0..NUM_BLOCKS {
        produce_block_with_tx(&mut rng, &fuel_client).await;
    }
    wait_for_aggregated_height(&fuel_client, BlockHeight::new(NUM_BLOCKS)).await;

    // when
    let stream = fuel_client
        .get_block_range(BlockHeight::new(1), BlockHeight::new(NUM_BLOCKS))
        .await
        .unwrap();
    futures::pin_mut!(stream);

    let mut heights = Vec::new();
    while let Some(next) = stream.next().await {
        let (block, _receipts) = next.unwrap();
        heights.push(*block.header().height());
    }

    // then
    let expected: Vec<BlockHeight> = (1..=NUM_BLOCKS).map(BlockHeight::new).collect();
    assert_eq!(heights, expected);

    clean_s3_bucket().await;
    drop(srv);
}

/// Multi-block coverage for the S3 (AWS SDK) fetch path. The existing single-block test covers
/// the happy path; this ensures the streamed range both in the gRPC response and the subsequent
/// per-block S3 GETs handle more than one element.
#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__s3_endpoint__streams_multi_block_range() {
    require_env_var_or_panic!("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION");
    ensure_bucket_exists().await;
    clean_s3_bucket().await;

    const NUM_BLOCKS: u32 = 3;
    let storage_method = StorageMethod::S3 {
        bucket: "test-bucket".to_string(),
        endpoint_url: Some(AWS_ENDPOINT_URL.to_string()),
        requester_pays: false,
        public_block_http_url: None,
        public_block_http_headers: Default::default(),
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let srv = FuelService::from_database(Database::default(), config)
        .await
        .unwrap();
    let fuel_client = FuelClient::new_with_rpc(
        iter::once(srv.bound_address.to_string()),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();

    let mut rng = StdRng::seed_from_u64(0xF00D);
    for _ in 0..NUM_BLOCKS {
        produce_block_with_tx(&mut rng, &fuel_client).await;
    }
    wait_for_aggregated_height(&fuel_client, BlockHeight::new(NUM_BLOCKS)).await;

    let stream = fuel_client
        .get_block_range(BlockHeight::new(1), BlockHeight::new(NUM_BLOCKS))
        .await
        .unwrap();
    futures::pin_mut!(stream);

    let mut heights = Vec::new();
    while let Some(next) = stream.next().await {
        let (block, _receipts) = next.unwrap();
        heights.push(*block.header().height());
    }

    let expected: Vec<BlockHeight> = (1..=NUM_BLOCKS).map(BlockHeight::new).collect();
    assert_eq!(heights, expected);

    clean_s3_bucket().await;
    drop(srv);
}

/// The server-side upload path should tag S3 objects with `Content-Encoding: gzip` so that both
/// the aws-sdk and raw HTTP clients see the encoding and decompress correctly. Without this, the
/// client decoder would feed gzipped bytes into the protobuf parser.
#[tokio::test(flavor = "multi_thread")]
async fn store_block__s3_object_carries_gzip_content_encoding_metadata() {
    require_env_var_or_panic!("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION");
    ensure_bucket_exists().await;
    clean_s3_bucket().await;

    let storage_method = StorageMethod::S3 {
        bucket: "test-bucket".to_string(),
        endpoint_url: Some(AWS_ENDPOINT_URL.to_string()),
        requester_pays: false,
        public_block_http_url: None,
        public_block_http_headers: Default::default(),
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let srv = FuelService::from_database(Database::default(), config)
        .await
        .unwrap();
    let fuel_client = FuelClient::new_with_rpc(
        iter::once(srv.bound_address.to_string()),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();

    let _ = fuel_client
        .submit_and_await_commit(&Transaction::default_test_tx())
        .await
        .unwrap();
    wait_for_aggregated_height(&fuel_client, BlockHeight::new(1)).await;

    // Inspect the raw S3 object metadata independently of the client's fetch path.
    let aws = aws_client().await;
    let key = block_height_to_key(&BlockHeight::new(1));
    let head = aws
        .head_object()
        .bucket("test-bucket")
        .key(&key)
        .send()
        .await
        .expect("block object should exist in S3");
    assert_eq!(head.content_encoding(), Some("gzip"));

    clean_s3_bucket().await;
    drop(srv);
}
