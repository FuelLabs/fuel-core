#![allow(non_snake_case)]

use aws_sdk_s3::{
    Client,
    config::{
        Credentials,
        Region,
    },
};
use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_block_aggregator_api::{
    blocks::importer_and_db_source::serializer_adapter::fuel_block_from_protobuf,
    db::{
        remote_cache::block_height_to_key,
        storage_or_remote_db::get_env_vars,
    },
    protobuf_types::{
        Block as ProtoBlock,
        BlockHeightRequest as ProtoBlockHeightRequest,
        BlockRangeRequest as ProtoBlockRangeRequest,
        NewBlockSubscriptionRequest as ProtoNewBlockSubscriptionRequest,
        RemoteBlockRangeResponse as ProtoRemoteBlockRangeResponse,
        block::VersionedBlock as ProtoVersionedBlock,
        block_aggregator_client::BlockAggregatorClient as ProtoBlockAggregatorClient,
        block_response::Payload as ProtoPayload,
        header::VersionedHeader as ProtoVersionedHeader,
    },
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::*,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use prost::bytes::Bytes;
use std::borrow::Cow;
use test_helpers::client_ext::ClientExt;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__can_get_serialized_block_from_rpc__literal() {
    if env_vars_are_set() {
        tracing::info!("Skipping test: AWS credentials are set");
        return;
    }
    let config = Config::local_node();
    let rpc_url = config.rpc_config.addr;

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

    // when
    let request = ProtoBlockRangeRequest { start: 1, end: 1 };
    let actual_block = if let Some(ProtoPayload::Literal(block)) = rpc_client
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
        block
    } else {
        panic!("expected literal block payload");
    };
    let ProtoVersionedBlock::V1(v1_block) = actual_block.versioned_block.unwrap();

    let actual_height = match v1_block.header.unwrap().versioned_header.unwrap() {
        ProtoVersionedHeader::V1(v1_header) => v1_header.height,
        ProtoVersionedHeader::V2(v2_header) => v2_header.height,
    };

    // then
    assert_eq!(expected_header.height.0, actual_height);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__can_get_serialized_block_from_rpc__remote() {
    let Some((_, _, aws_region, aws_bucket, url_base, _)) = get_env_vars() else {
        tracing::info!("Skipping test: AWS credentials are not set");
        return;
    };
    ensure_bucket_exists().await;
    clean_s3_bucket().await;
    let config = Config::local_node();
    let rpc_url = config.rpc_config.addr;

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
    let expected = ProtoRemoteBlockRangeResponse {
        region: aws_region.clone(),
        bucket: aws_bucket.clone(),
        key: key.clone(),
        url: format!("{}/blocks/{}", url_base, key),
    };
    assert_eq!(expected, remote_info);
    clean_s3_bucket().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_height__can_get_value_from_rpc() {
    if get_env_vars().is_some() {
        ensure_bucket_exists().await;
        clean_s3_bucket().await;
    }
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    let config = Config::local_node();
    let rpc_url = config.rpc_config.addr;

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
        .get_block_height(request)
        .await
        .unwrap()
        .into_inner()
        .height;

    // cleanup
    if get_env_vars().is_some() {
        clean_s3_bucket().await;
    }

    // then
    assert_eq!(expected_height, actual_height);
}

#[tokio::test(flavor = "multi_thread")]
async fn new_block_subscription__can_get_expect_block() {
    if get_env_vars().is_some() {
        ensure_bucket_exists().await;
        clean_s3_bucket().await;
    }
    let config = Config::local_node();
    let rpc_url = config.rpc_config.addr;

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = ProtoBlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

    let request = ProtoNewBlockSubscriptionRequest {};
    let mut stream = rpc_client
        .new_block_subscription(request)
        .await
        .unwrap()
        .into_inner();
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    // when
    let next = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
        .await
        .unwrap();
    let actual_block =
        if let Some(ProtoPayload::Literal(block)) = next.unwrap().unwrap().payload {
            block
        } else {
            panic!("expected literal block payload");
        };

    let ProtoVersionedBlock::V1(v1_block) = actual_block.versioned_block.unwrap();
    let actual_height = match v1_block.header.unwrap().versioned_header.unwrap() {
        ProtoVersionedHeader::V1(v1_header) => v1_header.height,
        ProtoVersionedHeader::V2(v2_header) => v2_header.height,
    };

    // then
    let expected_height = 1;
    assert_eq!(expected_height, actual_height);
    if get_env_vars().is_some() {
        clean_s3_bucket().await;
    }
}

macro_rules! require_env_var_or_skip {
    ($($var:literal),+) => {
        $(if std::env::var($var).is_err() {
            eprintln!("Skipping test: missing {}", $var);
            return;
        })+
    };
}

fn env_vars_are_set() -> bool {
    std::env::var("AWS_ACCESS_KEY_ID").is_ok()
        && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok()
        && std::env::var("AWS_REGION").is_ok()
        && std::env::var("AWS_BUCKET").is_ok()
        && std::env::var("AWS_ENDPOINT_URL").is_ok()
        && std::env::var("BUCKET_URL_BASE").is_ok()
}

fn aws_client() -> Client {
    let (aws_access_key_id, aws_secret_access_key, aws_region, _, _, aws_endpoint_url) =
        get_env_vars().unwrap();

    let mut builder = aws_sdk_s3::config::Builder::new();
    if let Some(aws_endpoint_url) = aws_endpoint_url {
        builder.set_endpoint_url(Some(aws_endpoint_url.clone()));
    }

    let config = builder
        .region(Region::new(Cow::Owned(aws_region.clone())))
        .credentials_provider(Credentials::new(
            aws_access_key_id,
            aws_secret_access_key,
            None,
            None,
            "block-aggregator",
        ))
        .behavior_version_latest()
        .build();
    aws_sdk_s3::Client::from_conf(config)
}

async fn get_block_from_s3_bucket() -> Bytes {
    let client = aws_client();
    let bucket = std::env::var("AWS_BUCKET").unwrap();
    let key = block_height_to_key(&BlockHeight::new(1));
    let req = client.get_object().bucket(&bucket).key(&key);
    let obj = req.send().await.unwrap();
    let message = format!(
        "should be able to get block from bucket: {} with key {}",
        bucket, key
    );
    obj.body.collect().await.expect(&message).into_bytes()
}

async fn ensure_bucket_exists() {
    let client = aws_client();
    let bucket = std::env::var("AWS_BUCKET").unwrap();
    let req = client.create_bucket().bucket(&bucket);
    let expect_message = format!("should be able to create bucket: {}", bucket);
    let _ = req.send().await.expect(&expect_message);
}

async fn clean_s3_bucket() {
    let client = aws_client();
    let bucket = std::env::var("AWS_BUCKET").unwrap();
    let req = client.list_objects().bucket(&bucket);
    let objs = req.send().await.unwrap();
    for obj in objs.contents.unwrap_or_default() {
        let req = client.delete_object().bucket(&bucket).key(obj.key.unwrap());
        let _ = req.send().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__can_get_from_remote_s3_bucket() {
    require_env_var_or_skip!(
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_REGION",
        "AWS_BUCKET",
        "AWS_ENDPOINT_URL",
        "BUCKET_URL_BASE"
    );
    ensure_bucket_exists().await;
    clean_s3_bucket().await;

    // given
    let config = Config::local_node();
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();
    let graphql_client = FuelClient::from(srv.bound_address);
    let tx = Transaction::default_test_tx();

    // when
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    sleep(std::time::Duration::from_secs(1)).await;

    // then
    let data = get_block_from_s3_bucket().await;
    // can deserialize
    let actual_proto: ProtoBlock = prost::Message::decode(data.as_ref()).unwrap();
    let _ = fuel_block_from_protobuf(actual_proto, &[], Bytes32::default()).unwrap();

    // cleanup
    clean_s3_bucket().await;
    drop(srv);
}
