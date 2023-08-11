use crate::import::{
    create_import,
    Count,
    Durations,
};
use fuel_core_services::SharedMutex;
use fuel_core_sync::state::State;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn test_v1() {
    let durations = Durations {
        headers: Duration::from_millis(5),
        consensus: Duration::from_millis(5),
        transactions: Duration::from_millis(5),
        executes: Duration::from_millis(10),
    };
    let n = 10usize;
    let buffer_size = 10;
    let state = State::new(None, n as u32);
    let shared_count = SharedMutex::new(Count::default());
    let shared_state = SharedMutex::new(state);
    let (import, _tx, mut shutdown) = create_import(
        shared_count.clone(),
        shared_state,
        durations,
        buffer_size,
        buffer_size,
    );
    import.notify_one();
    import.import(&mut shutdown).await.unwrap();
    println!("{:?}", shared_count);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_v2() {
    let durations = Durations {
        headers: Duration::from_millis(5),
        consensus: Duration::from_millis(5),
        transactions: Duration::from_millis(5),
        executes: Duration::from_millis(10),
    };
    let n = 10usize;
    let buffer_size = 10;
    let state = State::new(None, n as u32);
    let shared_count = SharedMutex::new(Count::default());
    let shared_state = SharedMutex::new(state);
    let (import, _tx, mut shutdown) = create_import(
        shared_count.clone(),
        shared_state,
        durations,
        buffer_size,
        buffer_size,
    );
    import.notify_one();
    import.import_v2(&mut shutdown).await.unwrap();
    println!("{:?}", shared_count);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_v3() {
    let durations = Durations {
        headers: Duration::from_millis(5),
        consensus: Duration::from_millis(5),
        transactions: Duration::from_millis(5),
        executes: Duration::from_millis(10),
    };
    let n = 10usize;
    let buffer_size = 10;
    let state = State::new(None, n as u32);
    let shared_count = SharedMutex::new(Count::default());
    let shared_state = SharedMutex::new(state);
    let (import, _tx, mut shutdown) = create_import(
        shared_count.clone(),
        shared_state,
        durations,
        buffer_size,
        buffer_size,
    );
    import.notify_one();
    import.import_v3(&mut shutdown).await.unwrap();
    println!("{:?}", shared_count);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_v4() {
    let durations = Durations {
        headers: Duration::from_millis(5),
        consensus: Duration::from_millis(5),
        transactions: Duration::from_millis(5),
        executes: Duration::from_millis(10),
    };
    let n = 10usize;
    let buffer_size = 10;
    let state = State::new(None, n as u32);
    let shared_count = SharedMutex::new(Count::default());
    let shared_state = SharedMutex::new(state);
    let (import, _tx, mut shutdown) = create_import(
        shared_count.clone(),
        shared_state,
        durations,
        buffer_size,
        buffer_size,
    );
    import.notify_one();
    import.import_v4(&mut shutdown).await.unwrap();
    println!("{:?}", shared_count);
}
