use anyhow::Error;
use ethers_core::types::{Filter, Address,H160, ValueOrArray};

use ethers_providers::{Middleware, Provider, StreamExt, Ws, FilterKind};
use std::{convert::TryFrom, str::FromStr, time::Duration};

use env_logger::Builder;
use std::env;

#[tokio::main]
async fn main() -> Result<(),Error> {
    Builder::new()
        .parse_env(&env::var("MY_APP_LOG").unwrap_or_default())
        .init();

    
        let ws = Ws::connect("wss://mainnet.infura.io/ws/v3/0954246eab5544e89ac236b668980810").await?;
        let provider = Provider::new(ws).interval(Duration::from_millis(1000));

    /*
        From infure: newBlockFilter
        Creates a filter in the node, to notify when a new block arrives. To check if the state has changed, call eth_getFilterChanges.
        Filter IDs will be valid for up to fifteen minutes, and can polled by any connection using the same v3 project ID.
    */
    // it seems thaat provider periodically featched data every 7s (maybe high for us)
    /* seems good intro: https://tms-dev-blog.com/rust-web3-connect-to-ethereum/ */
    /* examples: https://github.com/gakonst/ethers-rs/tree/master/examples */
    /* it seems ethers was inspired by web3 here are examples that can be useful: https://github.com/tomusdrw/rust-web3/tree/master/examples */

    {
        //let block_watcher = provider.watch_blocks().await.expect("all good");
        //let mut stream = block_watcher.stream();
        // while let Some(block_hash) = stream.next().await {
        //     println!("received block: {:?}", block_hash);
        //     let block = provider.get_block(block_hash).await.expect("BLOCK");
        //     println!("Block received:{:?}", block);
        // }
    }
    // address for wETH
    let filter = Filter::new().address(ValueOrArray::Value(H160::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap()));
    println!("TEST");
    let t = provider.new_filter(FilterKind::Logs(&filter)).await.expect("TO WORK");
    //tokio::time::sleep(Duration::from_millis(10000)).await;
    //println!("Get any logs:{:?}",provider.get_logs(&filter).await.expect("to work"));


    let mut logs_watcher = provider.watch(&filter).await.expect("to work");

    while let Some(logs) = logs_watcher.next().await {
        println!("received LOG: {:?}", logs);
    }
    println!("END");

    //provider.w

    // let block = provider.get_block(100u64).await.expect("be good");
    // println!(
    //     "Got block: {}",
    //     serde_json::to_string(&block).expect("be string")
    // );

    Ok(())
}
