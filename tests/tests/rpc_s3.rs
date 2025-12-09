#![allow(non_snake_case)]

use aws_config::{
    BehaviorVersion,
    default_provider::credentials::DefaultCredentialsChain,
};
use aws_sdk_s3::Client;
use flate2::read::GzDecoder;
use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_block_aggregator_api::{
    blocks::importer_and_db_source::serializer_adapter::proto_to_fuel_conversions::fuel_block_from_protobuf,
    db::remote_cache::block_height_to_key,
    protobuf_types::{
        Block as ProtoBlock,
        BlockHeightRequest as ProtoBlockHeightRequest,
        BlockRangeRequest as ProtoBlockRangeRequest,
        RemoteBlockResponse as ProtoRemoteBlockResponse,
        RemoteS3Bucket,
        block_aggregator_client::BlockAggregatorClient as ProtoBlockAggregatorClient,
        block_response::Payload as ProtoPayload,
        remote_block_response::Location,
    },
    service::StorageMethod,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::*,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use prost::bytes::Bytes;
use std::io::Read;
use test_helpers::client_ext::ClientExt;
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
async fn get_block_range__can_get_serialized_block_from_rpc__remote() {
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
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let rpc_url = config.rpc_config.clone().unwrap().addr;

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = ProtoBlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

    let expected_block = graphql_client
        .full_block_by_height(1)
        .await
        .unwrap()
        .unwrap();
    let expected_header = expected_block.header;
    let expected_height = BlockHeight::new(expected_header.height.0);

    // when
    let request = ProtoBlockRangeRequest { start: 1, end: 1 };
    let remote_info = if let Some(ProtoPayload::Remote(remote_info)) = rpc_client
        .get_block_range(request)
        .await
        .unwrap()
        .into_inner()
        .next()
        .await
        .unwrap()
        .unwrap()
        .payload
    {
        remote_info
    } else {
        panic!("expected literal block payload");
    };

    // then
    let key = block_height_to_key(&expected_height);
    let expected = ProtoRemoteBlockResponse {
        location: Some(Location::S3(RemoteS3Bucket {
            bucket: "test-bucket".to_string(),
            key,
            requester_pays: false,
            endpoint: Some(AWS_ENDPOINT_URL.to_string()),
        })),
    };
    assert_eq!(expected, remote_info);

    // cleanup
    clean_s3_bucket().await;
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
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let rpc_url = config.rpc_config.clone().unwrap().addr;

    // given
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = ProtoBlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

    // when
    sleep(std::time::Duration::from_secs(1)).await;
    let request = ProtoBlockHeightRequest {};
    let expected_height = Some(1);
    let actual_height = rpc_client
        .get_synced_block_height(request)
        .await
        .unwrap()
        .into_inner()
        .height;

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

async fn get_block_from_s3_bucket() -> Bytes {
    let client = aws_client().await;
    let bucket = "test-bucket".to_string();
    let key = block_height_to_key(&BlockHeight::new(1));
    tracing::info!("getting block from bucket: {} with key {}", bucket, key);
    let req = client.get_object().bucket(&bucket).key(&key);
    let obj = req.send().await.unwrap();
    let message = format!(
        "should be able to get block from bucket: {} with key {}",
        bucket, key
    );
    obj.body.collect().await.expect(&message).into_bytes()
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
    let zipped_data = get_block_from_s3_bucket().await;
    let data = unzip_bytes(&zipped_data);
    let actual_proto: ProtoBlock = prost::Message::decode(data.as_ref()).unwrap();
    let (_, receipts) =
        fuel_block_from_protobuf(actual_proto, &[], Bytes32::default()).unwrap();
    assert!(
        matches!(
            receipts[1],
            Receipt::ScriptResult {
                result: ScriptExecutionResult::Success,
                ..
            }
        ),
        "should have a script result receipt, received: {:?}",
        receipts
    );
    assert!(
        matches!(receipts[0], Receipt::Return { .. }),
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

fn unzip_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut decoder = GzDecoder::new(bytes);
    let mut output = Vec::new();
    decoder.read_to_end(&mut output).unwrap();
    output
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__no_publish__can_get_block_info_from_rpc__remote() {
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
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let rpc_url = config.rpc_config.clone().unwrap().addr;

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = ProtoBlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

    let expected_block = graphql_client
        .full_block_by_height(1)
        .await
        .unwrap()
        .unwrap();
    let expected_header = expected_block.header;
    let expected_height = BlockHeight::new(expected_header.height.0);

    // when
    let request = ProtoBlockRangeRequest { start: 1, end: 1 };
    let remote_info = if let Some(ProtoPayload::Remote(remote_info)) = rpc_client
        .get_block_range(request)
        .await
        .unwrap()
        .into_inner()
        .next()
        .await
        .unwrap()
        .unwrap()
        .payload
    {
        remote_info
    } else {
        panic!("expected literal block payload");
    };

    // then
    let key = block_height_to_key(&expected_height);
    let expected = ProtoRemoteBlockResponse {
        location: Some(Location::S3(RemoteS3Bucket {
            bucket: "test-bucket".to_string(),
            key,
            requester_pays: false,
            endpoint: Some(AWS_ENDPOINT_URL.to_string()),
        })),
    };
    assert_eq!(expected, remote_info);

    // cleanup
    clean_s3_bucket().await;
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
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);
    let rpc_url = config.rpc_config.clone().unwrap().addr;

    // given
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = ProtoBlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

    // when
    sleep(std::time::Duration::from_secs(1)).await;
    let request = ProtoBlockHeightRequest {};
    let expected_height = Some(1);
    let actual_height = rpc_client
        .get_synced_block_height(request)
        .await
        .unwrap()
        .into_inner()
        .height;

    // then
    assert_eq!(expected_height, actual_height);

    // cleanup
    clean_s3_bucket().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__no_publish__does_not_publish_to_s3_bucket() {
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
