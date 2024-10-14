//! Stream that yields each `batch_size` items allowing other tasks to work.

use futures::{
    ready,
    stream::Fuse,
    Stream,
    StreamExt,
};
use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

pin_project_lite::pin_project! {
    /// Stream that yields each `batch_size` items.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct YieldStream<St: Stream> {
        #[pin]
        stream: Fuse<St>,
        item: Option<St::Item>,
        counter: usize,
        batch_size: usize,
    }
}

impl<St: Stream> YieldStream<St> {
    /// Create a new `YieldStream` with the given `batch_size`.
    pub fn new(stream: St, batch_size: usize) -> Self {
        assert!(batch_size > 0);

        Self {
            stream: stream.fuse(),
            item: None,
            counter: 0,
            batch_size,
        }
    }
}

impl<St: Stream> Stream for YieldStream<St> {
    type Item = St::Item;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();
        loop {
            // If we have a cached item, return it because that means we were woken up.
            if let Some(item) = this.item.take() {
                *this.counter = 1;
                return Poll::Ready(Some(item));
            }

            match ready!(this.stream.as_mut().poll_next(cx)) {
                // Return items, unless we reached the batch size.
                // after that, we want to yield before returning the next item.
                Some(item) => {
                    if this.counter < this.batch_size {
                        *this.counter = this.counter.saturating_add(1);

                        return Poll::Ready(Some(item));
                    } else {
                        *this.item = Some(item);

                        cx.waker().wake_by_ref();

                        return Poll::Pending;
                    }
                }

                // Underlying stream ran out of values, so finish this stream as well.
                None => {
                    return Poll::Ready(None);
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let cached_len = usize::from(self.item.is_some());
        let (lower, upper) = self.stream.size_hint();
        let lower = lower.saturating_add(cached_len);
        let upper = match upper {
            Some(x) => x.checked_add(cached_len),
            None => None,
        };
        (lower, upper)
    }
}

/// Extension trait for `Stream`.
pub trait StreamYieldExt: Stream {
    /// Yields each `batch_size` items allowing other tasks to work.
    fn yield_each(self, batch_size: usize) -> YieldStream<Self>
    where
        Self: Sized,
    {
        YieldStream::new(self, batch_size)
    }
}

impl<St> StreamYieldExt for St where St: Stream {}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn yield_stream__works_with_10_elements_loop() {
        let stream = futures::stream::iter(0..10);
        let mut yield_stream = YieldStream::new(stream, 3);

        let mut items = Vec::new();
        while let Some(item) = yield_stream.next().await {
            items.push(item);
        }

        assert_eq!(items, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[tokio::test]
    async fn yield_stream__works_with_10_elements__collect() {
        let stream = futures::stream::iter(0..10);
        let yield_stream = stream.yield_each(3);

        let items = yield_stream.collect::<Vec<_>>().await;

        assert_eq!(items, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[tokio::test]
    async fn yield_stream__passed_control_to_another_future() {
        let stream = futures::stream::iter(0..10);
        let mut yield_stream = YieldStream::new(stream, 3);

        async fn second_future() -> i32 {
            -1
        }

        let mut items = Vec::new();
        loop {
            tokio::select! {
                biased;

                item = yield_stream.next() => {
                    if let Some(item) = item {
                        items.push(item);
                    } else {
                        break;
                    }
                }

                item = second_future() => {
                    items.push(item);
                }
            }
        }

        assert_eq!(items, vec![0, 1, 2, -1, 3, 4, 5, -1, 6, 7, 8, -1, 9]);
    }
}
