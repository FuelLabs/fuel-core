mod arweave_service;
mod ipfs_service;
mod metrics;

use std::collections::HashMap;
use std::fmt::Display;
use std::hash::Hash;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::StreamExt;
use futures::{stream, Future, FutureExt, TryFutureExt};
use graph::cheap_clone::CheapClone;
use graph::parking_lot::Mutex;
use graph::prelude::tokio;
use graph::prometheus::{Counter, Gauge};
use graph::slog::{debug, Logger};
use graph::util::monitored::MonitoredVecDeque as VecDeque;
use tokio::sync::{mpsc, watch};
use tower::retry::backoff::{Backoff, ExponentialBackoff, ExponentialBackoffMaker, MakeBackoff};
use tower::util::rng::HasherRng;
use tower::{Service, ServiceExt};

pub use self::metrics::PollingMonitorMetrics;
pub use arweave_service::{arweave_service, ArweaveService};
pub use ipfs_service::{ipfs_service, IpfsService};

const MIN_BACKOFF: Duration = Duration::from_secs(5);

const MAX_BACKOFF: Duration = Duration::from_secs(600);

struct Backoffs<ID> {
    backoff_maker: ExponentialBackoffMaker,
    backoffs: HashMap<ID, ExponentialBackoff>,
}

impl<ID: Eq + Hash> Backoffs<ID> {
    fn new() -> Self {
        // Unwrap: Config is constant and valid.
        Self {
            backoff_maker: ExponentialBackoffMaker::new(
                MIN_BACKOFF,
                MAX_BACKOFF,
                1.0,
                HasherRng::new(),
            )
            .unwrap(),
            backoffs: HashMap::new(),
        }
    }

    fn next_backoff(&mut self, id: ID) -> impl Future<Output = ()> {
        self.backoffs
            .entry(id)
            .or_insert_with(|| self.backoff_maker.make_backoff())
            .next_backoff()
    }

    fn remove(&mut self, id: &ID) {
        self.backoffs.remove(id);
    }
}

// A queue that notifies `waker` whenever an element is pushed.
struct Queue<T> {
    queue: Mutex<VecDeque<T>>,
    waker: watch::Sender<()>,
}

impl<T> Queue<T> {
    fn new(depth: Gauge, popped: Counter) -> (Arc<Self>, watch::Receiver<()>) {
        let queue = Mutex::new(VecDeque::new(depth, popped));
        let (waker, woken) = watch::channel(());
        let this = Queue { queue, waker };
        (Arc::new(this), woken)
    }

    fn push_back(&self, e: T) {
        self.queue.lock().push_back(e);
        let _ = self.waker.send(());
    }

    fn push_front(&self, e: T) {
        self.queue.lock().push_front(e);
        let _ = self.waker.send(());
    }

    fn pop_front(&self) -> Option<T> {
        self.queue.lock().pop_front()
    }
}

/// Spawn a monitor that actively polls a service. Whenever the service has capacity, the monitor
/// pulls object ids from the queue and polls the service. If the object is not present or in case
/// of error, the object id is pushed to the back of the queue to be polled again.
///
/// The service returns the request ID along with errors or responses. The response is an
/// `Option`, to represent the object not being found.
pub fn spawn_monitor<ID, S, E, Res: Send + 'static>(
    service: S,
    response_sender: mpsc::UnboundedSender<(ID, Res)>,
    logger: Logger,
    metrics: Arc<PollingMonitorMetrics>,
) -> PollingMonitor<ID>
where
    S: Service<ID, Response = Option<Res>, Error = E> + Send + 'static,
    ID: Display + Clone + Default + Eq + Send + Sync + Hash + 'static,
    E: Display + Send + 'static,
    S::Future: Send,
{
    let service = ReturnRequest { service };
    let (queue, queue_woken) = Queue::new(metrics.queue_depth.clone(), metrics.requests.clone());

    let cancel_check = response_sender.clone();
    let queue_to_stream = {
        let queue = queue.cheap_clone();
        stream::unfold((), move |()| {
            let queue = queue.cheap_clone();
            let mut queue_woken = queue_woken.clone();
            let cancel_check = cancel_check.clone();
            async move {
                loop {
                    if cancel_check.is_closed() {
                        break None;
                    }

                    let id = queue.pop_front();
                    match id {
                        Some(id) => break Some((id, ())),

                        // Nothing on the queue, wait for a queue wake up or cancellation.
                        None => {
                            futures::future::select(
                                // Unwrap: `queue` holds a sender.
                                queue_woken.changed().map(|r| r.unwrap()).boxed(),
                                cancel_check.closed().boxed(),
                            )
                            .await;
                        }
                    }
                }
            }
        })
    };

    {
        let queue = queue.cheap_clone();
        graph::spawn(async move {
            let mut backoffs = Backoffs::new();
            let mut responses = service.call_all(queue_to_stream).unordered().boxed();
            while let Some(response) = responses.next().await {
                // Note: Be careful not to `await` within this loop, as that could block requests in
                // the `CallAll` from being polled. This can cause starvation as those requests may
                // be holding on to resources such as slots for concurrent calls.
                match response {
                    Ok((id, Some(response))) => {
                        backoffs.remove(&id);
                        let send_result = response_sender.send((id, response));
                        if send_result.is_err() {
                            // The receiver has been dropped, cancel this task.
                            break;
                        }
                    }

                    // Object not found, push the id to the back of the queue.
                    Ok((id, None)) => {
                        debug!(logger, "not found on polling"; "object_id" => id.to_string());

                        metrics.not_found.inc();
                        queue.push_back(id);
                    }

                    // Error polling, log it and push the id to the back of the queue.
                    Err((id, e)) => {
                        debug!(logger, "error polling";
                                    "error" => format!("{:#}", e),
                                    "object_id" => id.to_string());
                        metrics.errors.inc();

                        // Requests that return errors could mean there is a permanent issue with
                        // fetching the given item, or could signal the endpoint is overloaded.
                        // Either way a backoff makes sense.
                        let queue = queue.cheap_clone();
                        let backoff = backoffs.next_backoff(id.clone());
                        graph::spawn(async move {
                            backoff.await;
                            queue.push_back(id);
                        });
                    }
                }
            }
        });
    }

    PollingMonitor { queue }
}

/// Handle for adding objects to be monitored.
pub struct PollingMonitor<ID> {
    queue: Arc<Queue<ID>>,
}

impl<ID> PollingMonitor<ID> {
    /// Add an object id to the polling queue. New requests have priority and are pushed to the
    /// front of the queue.
    pub fn monitor(&self, id: ID) {
        self.queue.push_front(id);
    }
}

struct ReturnRequest<S> {
    service: S,
}

impl<S, Req> Service<Req> for ReturnRequest<S>
where
    S: Service<Req>,
    Req: Clone + Default + Send + Sync + 'static,
    S::Error: Send,
    S::Future: Send + 'static,
{
    type Response = (Req, S::Response);
    type Error = (Req, S::Error);
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        // `Req::default` is a value that won't be used since if `poll_ready` errors, the service is shot anyways.
        self.service.poll_ready(cx).map_err(|e| (Req::default(), e))
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let req1 = req.clone();
        self.service
            .call(req.clone())
            .map_ok(move |x| (req, x))
            .map_err(move |e| (req1, e))
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use graph::log;
    use tower_test::mock;

    use super::*;

    async fn send_response<T, U>(handle: &mut mock::Handle<T, U>, res: U) {
        handle.next_request().await.unwrap().1.send_response(res)
    }

    fn setup() -> (
        mock::Handle<&'static str, Option<&'static str>>,
        PollingMonitor<&'static str>,
        mpsc::UnboundedReceiver<(&'static str, &'static str)>,
    ) {
        let (svc, handle) = mock::pair();
        let (tx, rx) = mpsc::unbounded_channel();
        let monitor = spawn_monitor(
            svc,
            tx,
            log::discard(),
            Arc::new(PollingMonitorMetrics::mock()),
        );
        (handle, monitor, rx)
    }

    #[tokio::test]
    async fn polling_monitor_shared_svc() {
        let (svc, mut handle) = mock::pair();
        let shared_svc = tower::buffer::Buffer::new(tower::limit::ConcurrencyLimit::new(svc, 1), 1);
        let make_monitor = |svc| {
            let (tx, rx) = mpsc::unbounded_channel();
            let metrics = Arc::new(PollingMonitorMetrics::mock());
            let monitor = spawn_monitor(svc, tx, log::discard(), metrics);
            (monitor, rx)
        };

        // Spawn a monitor and yield to ensure it is polled and waiting on the tx.
        let (_monitor0, mut _rx0) = make_monitor(shared_svc.clone());
        tokio::task::yield_now().await;

        // Test that the waiting monitor above is not occupying a concurrency slot on the service.
        let (monitor1, mut rx1) = make_monitor(shared_svc);
        monitor1.monitor("req-0");
        send_response(&mut handle, Some("res-0")).await;
        assert_eq!(rx1.recv().await, Some(("req-0", "res-0")));
    }

    #[tokio::test]
    async fn polling_monitor_simple() {
        let (mut handle, monitor, mut rx) = setup();

        // Basic test, single file is immediately available.
        monitor.monitor("req-0");
        send_response(&mut handle, Some("res-0")).await;
        assert_eq!(rx.recv().await, Some(("req-0", "res-0")));
    }

    #[tokio::test]
    async fn polling_monitor_unordered() {
        let (mut handle, monitor, mut rx) = setup();

        // Test unorderedness of the response stream, and the LIFO semantics of `monitor`.
        //
        // `req-1` has priority since it is the last request, but `req-0` is responded first.
        monitor.monitor("req-0");
        monitor.monitor("req-1");
        let req_1 = handle.next_request().await.unwrap().1;
        let req_0 = handle.next_request().await.unwrap().1;
        req_0.send_response(Some("res-0"));
        assert_eq!(rx.recv().await, Some(("req-0", "res-0")));
        req_1.send_response(Some("res-1"));
        assert_eq!(rx.recv().await, Some(("req-1", "res-1")));
    }

    #[tokio::test]
    async fn polling_monitor_failed_push_to_back() {
        let (mut handle, monitor, mut rx) = setup();

        // Test that objects not found go on the back of the queue.
        monitor.monitor("req-0");
        monitor.monitor("req-1");
        send_response(&mut handle, None).await;
        send_response(&mut handle, Some("res-0")).await;
        assert_eq!(rx.recv().await, Some(("req-0", "res-0")));
        send_response(&mut handle, Some("res-1")).await;
        assert_eq!(rx.recv().await, Some(("req-1", "res-1")));

        // Test that failed requests go on the back of the queue.
        monitor.monitor("req-0");
        monitor.monitor("req-1");
        let req = handle.next_request().await.unwrap().1;
        req.send_error(anyhow!("e"));
        send_response(&mut handle, Some("res-0")).await;
        assert_eq!(rx.recv().await, Some(("req-0", "res-0")));
        send_response(&mut handle, Some("res-1")).await;
        assert_eq!(rx.recv().await, Some(("req-1", "res-1")));
    }

    #[tokio::test]
    async fn polling_monitor_cancelation() {
        // Cancelation on receiver drop, no pending request.
        let (mut handle, _monitor, rx) = setup();
        drop(rx);
        assert!(handle.next_request().await.is_none());

        // Cancelation on receiver drop, with pending request.
        let (mut handle, monitor, rx) = setup();
        monitor.monitor("req-0");
        drop(rx);
        assert!(handle.next_request().await.is_none());

        // Cancelation on receiver drop, while queue is waiting.
        let (mut handle, _monitor, rx) = setup();
        let handle = tokio::spawn(async move { handle.next_request().await });
        tokio::task::yield_now().await;
        drop(rx);
        assert!(handle.await.unwrap().is_none());
    }
}
