use anyhow::Error;
use graph::{
    endpoint::EndpointMetrics,
    env::env_var,
    firehose::SubgraphLimit,
    log::logger,
    prelude::{prost, tokio, tonic},
    {firehose, firehose::FirehoseEndpoint},
};
use prost::Message;
use std::sync::Arc;
use tonic::Streaming;
use graph::blockchain::Block;
use graph_chain_fuel::codec;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut cursor: Option<String> = None;
    let token_env = env_var("SF_API_TOKEN", "".to_string());
    let mut token: Option<String> = None;
    if !token_env.is_empty() {
        token = Some(token_env);
    }

    let logger = logger(false);
    let host = "http://localhost:10015".to_string();

    let firehose = Arc::new(FirehoseEndpoint::new(
        "firehose",
        &host,
        token,
        false,
        false,
        SubgraphLimit::Unlimited,
        Arc::new(EndpointMetrics::mock()),
    ));

    loop {
        println!("Connecting to the stream!");
        let mut stream: Streaming<firehose::Response> = match firehose
            .clone()
            .stream_blocks(firehose::Request {
                start_block_num: 1,
                stop_block_num: 5,
                cursor: match &cursor {
                    Some(c) => c.clone(),
                    None => String::from(""),
                },
                final_blocks_only: false,
                ..Default::default()
            })
            .await
        {
            Ok(s) => s,
            Err(e) => {
                println!("Could not connect to stream! {}", e);
                continue;
            }
        };

        loop {
            let resp = match stream.message().await {
                Ok(Some(t)) => t,
                Ok(None) => {
                    println!("Stream completed");
                    return Ok(());
                }
                Err(e) => {
                    println!("Error getting message {}", e);
                    break;
                }
            };

            let b = codec::Block::decode(resp.block.unwrap().value.as_ref());
            match b {
                Ok(b) => {
                    println!(
                        "Block #{} ({}) ({})",
                        b.number(),
                        b.hash(),
                        resp.step
                    );
                    cursor = Some(resp.cursor)
                }
                Err(e) => panic!("Unable to decode {:?}", e),
            }
        }
    }
}

