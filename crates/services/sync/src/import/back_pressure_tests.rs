#![allow(clippy::arithmetic_side_effects)]
use std::time::Duration;

use super::*;
use crate::import::test_helpers::{
    Count,
    PressureBlockImporter,
    PressureConsensus,
    PressurePeerToPeer,
    SharedCounts,
};
use test_case::test_case;

#[derive(Default)]
struct Input {
    headers: Duration,
    consensus: Duration,
    transactions: Duration,
    executes: Duration,
}

#[test_case(
    Input::default(), State::new(None, None),
    Config{
        block_stream_buffer_size: 1,
        header_batch_size: 1,
    }
    => Count::default() ; "Empty sanity test"
)]
#[test_case(
    Input {
        headers: Duration::from_millis(10),
        ..Default::default()
    },
    State::new(None, 0),
    Config{
        block_stream_buffer_size: 1,
        header_batch_size: 1,
    }
    => is less_or_equal_than Count{ headers: 1, consensus: 1, transactions: 1, executes: 1, blocks: 1 }
    ; "Single with slow headers"
)]
#[test_case(
    Input {
        headers: Duration::from_millis(10),
        ..Default::default()
    },
    State::new(None, 1000),
    Config{
        block_stream_buffer_size: 10,
        header_batch_size: 5,
    }
    => is less_or_equal_than Count{ headers: 10, consensus: 10, transactions: 10, executes: 1, blocks: 50 }
    ; "1000 headers with max 5 size and max 10 requests when slow headers"
)]
#[test_case(
    Input {
        transactions: Duration::from_millis(10),
        ..Default::default()
    },
    State::new(None, 1000),
    Config{
        block_stream_buffer_size: 10,
        header_batch_size: 5,
    }
    => is less_or_equal_than Count{ headers: 10, consensus: 10, transactions: 10, executes: 1, blocks: 50 }
    ; "1000 headers with max 5 size and max 10 requests when transactions"
)]
#[test_case(
    Input {
        consensus: Duration::from_millis(10),
        ..Default::default()
    },
    State::new(None, 1000),
    Config{
        block_stream_buffer_size: 10,
        header_batch_size: 5,
    }
    => is less_or_equal_than Count{ headers: 10, consensus: 10, transactions: 10, executes: 1, blocks: 50 }
    ; "1000 headers with max 5 size and max 10 requests when consensus"
)]
#[test_case(
    Input {
        executes: Duration::from_millis(10),
        ..Default::default()
    },
    State::new(None, 1000),
    Config{
        block_stream_buffer_size: 10,
        header_batch_size: 5,
    }
    => is less_or_equal_than Count{ headers: 10, consensus: 10, transactions: 10, executes: 1, blocks: 60 }
    ; "1000 headers with max 5 size and max 10 requests when execution is slow. \
        50 blocks should be fetched; 5 awaits pushing to execution; 5 is executing;"
)]
#[tokio::test(flavor = "multi_thread")]
async fn test_back_pressure(input: Input, state: State, params: Config) -> Count {
    let counts = SharedCounts::new(Default::default());
    let state = SharedMutex::new(state);

    let p2p = Arc::new(PressurePeerToPeer::new(
        counts.clone(),
        [input.headers, input.transactions],
    ));
    let executor = Arc::new(PressureBlockImporter::new(counts.clone(), input.executes));
    let consensus = Arc::new(PressureConsensus::new(counts.clone(), input.consensus));
    let notify = Arc::new(Notify::new());

    let import = Import {
        state,
        notify,
        params,
        p2p,
        executor,
        consensus,
    };

    import.notify.notify_one();
    let (_tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let mut watcher = shutdown.into();
    import.import(&mut watcher).await.unwrap();
    counts.apply(|c| c.max.clone())
}
