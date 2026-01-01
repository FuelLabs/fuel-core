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
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);

    // given
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let fuel_client = FuelClient::new_with_rpc(
        srv.bound_address.to_string(),
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
    let fuel_client = FuelClient::new_with_rpc(
        srv.bound_address.to_string(),
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
    };
    let config = Config::local_node_with_rpc_and_storage_method(storage_method);

    // given
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let fuel_client = FuelClient::new_with_rpc(
        srv.bound_address.to_string(),
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
