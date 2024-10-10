use crate::futures::{
    future_tracker::FutureTracker,
    FuturesMetrics,
};
use std::{
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

pin_project_lite::pin_project! {
    /// Future that tracks its execution with [`FutureTracker`] and reports it back when done
    /// to [`FuturesMetrics`].
    #[derive(Debug, Clone)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct MeteredFuture<F> {
        #[pin]
        future: FutureTracker<F>,
        metrics: FuturesMetrics,
    }
}

impl<F> MeteredFuture<F> {
    /// Create a new `MeteredFuture` with the given future and metrics.
    pub fn new(future: F, metrics: FuturesMetrics) -> Self {
        Self {
            future: FutureTracker::new(future),
            metrics,
        }
    }
}

impl<F> Future for MeteredFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.future.poll(cx) {
            Poll::Ready(output) => {
                let output = output.extract(this.metrics);
                Poll::Ready(output)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn hybrid_time_correct() {
        let future = async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            std::thread::sleep(Duration::from_secs(1));
        };
        let metrics = FuturesMetrics::obtain_futures_metrics("test");
        let wrapper_future = MeteredFuture::new(future, metrics.clone());
        let _ = wrapper_future.await;
        let busy = Duration::from_nanos(metrics.busy.get());
        let idle = Duration::from_nanos(metrics.idle.get());
        assert_eq!(idle.as_secs(), 2);
        assert_eq!(busy.as_secs(), 1);
    }
}
