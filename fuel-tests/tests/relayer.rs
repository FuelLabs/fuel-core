use std::{
    net::Ipv4Addr,
    sync::Arc,
    time::Duration,
};

use ethers::providers::Middleware;
use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::db::Messages;
use fuel_gql_client::prelude::StorageAsRef;

use fuel_relayer::test_helpers::{
    middleware::MockMiddleware,
    EvtToLog,
    LogTestHelper,
};
use hyper::{
    service::{
        make_service_fn,
        service_fn,
    },
    Body,
    Request,
    Response,
    Server,
};
use serde_json::json;
use std::{
    convert::Infallible,
    net::SocketAddr,
};

async fn handle(
    mock: Arc<MockMiddleware>,
    req: Request<Body>,
) -> Result<Response<Body>, Infallible> {
    let body = hyper::body::to_bytes(req).await.unwrap();

    let v: serde_json::Value = serde_json::from_slice(body.as_ref()).unwrap();
    let mut o = match v {
        serde_json::Value::Object(o) => o,
        _ => unreachable!(),
    };
    let id = o.get("id").unwrap().as_u64().unwrap();
    let method = o.get("method").unwrap().as_str().unwrap();
    let r = match method {
        "eth_blockNumber" => {
            let r = mock.get_block_number().await.unwrap();
            json!({ "id": id, "jsonrpc": "2.0", "result": r })
        }
        "eth_syncing" => {
            let r = mock.syncing().await.unwrap();
            match r {
                ethers::providers::SyncingStatus::IsFalse => {
                    json!({ "id": id, "jsonrpc": "2.0", "result": false })
                }
                ethers::providers::SyncingStatus::IsSyncing {
                    starting_block,
                    current_block,
                    highest_block,
                } => {
                    json!({ "id": id, "jsonrpc": "2.0", "result": {
                        "starting_block": starting_block, 
                        "current_block": current_block,
                        "highest_block": highest_block,
                    } })
                }
            }
        }
        "eth_getLogs" => {
            let params = o.remove("params").unwrap();
            let params: Vec<_> = serde_json::from_value(params).unwrap();
            let r = mock.get_logs(&params[0]).await.unwrap();
            json!({ "id": id, "jsonrpc": "2.0", "result": r })
        }
        _ => unreachable!(),
    };

    let r = serde_json::to_vec(&r).unwrap();

    Ok(Response::new(Body::from(r)))
}

#[tokio::test(flavor = "multi_thread")]
async fn relayer_can_download_logs() {
    let mut config = Config::local_node();
    let eth_node = MockMiddleware::default();
    let contract_address = config.relayer.eth_v2_listening_contracts[0];
    let message = |nonce, block_number: u64| {
        let message = fuel_relayer::bridge::SentMessageFilter {
            nonce,
            ..Default::default()
        };
        let mut log = message.into_log();
        log.address = contract_address;
        log.block_number = Some(block_number.into());
        log
    };

    let logs = vec![message(1, 3), message(2, 5)];
    let expected_messages: Vec<_> = logs.iter().map(|l| l.to_msg()).collect();
    eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(200.into()));
    let eth_node = Arc::new(eth_node);

    // Construct our SocketAddr to listen on...
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));

    // And a MakeService to handle each connection...
    let make_service = make_service_fn(move |_conn| {
        let eth_node = eth_node.clone();
        async move {
            Ok::<_, Infallible>(service_fn({
                let eth_node = eth_node.clone();
                move |req| handle(eth_node.clone(), req)
            }))
        }
    });

    // Then bind and serve...
    let server = Server::bind(&addr).serve(make_service);
    let addr = server.local_addr();

    let (shutdown, rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let graceful = server.with_graceful_shutdown(async {
            rx.await.ok();
        });
        // And run forever...
        if let Err(e) = graceful.await {
            eprintln!("server error: {}", e);
        }
    });

    config.relayer.eth_client =
        Some(format!("http://{}", addr).as_str().try_into().unwrap());
    let db = Database::in_memory();

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(10)).await;
    for msg in expected_messages {
        assert_eq!(
            &*db.storage::<Messages>().get(msg.id()).unwrap().unwrap(),
            &*msg
        );
    }
    srv.stop().await;
    shutdown.send(()).unwrap();
}
