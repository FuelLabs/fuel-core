use std::sync::Arc;

use fuel_core_services::StateWatcher;
use futures::{
    StreamExt,
    TryStreamExt,
};
use itertools::Itertools;
use tokio::task::JoinSet;

pub struct TaskManager<T> {
    set: JoinSet<anyhow::Result<T>>,
    cancel_token: CancellationToken,
}

#[async_trait::async_trait]
pub trait NotifyCancel {
    async fn wait_until_cancelled(&self) -> anyhow::Result<()>;
    fn is_cancelled(&self) -> bool;
}

#[async_trait::async_trait]
impl NotifyCancel for tokio_util::sync::CancellationToken {
    async fn wait_until_cancelled(&self) -> anyhow::Result<()> {
        self.cancelled().await;
        Ok(())
    }
    fn is_cancelled(&self) -> bool {
        self.is_cancelled()
    }
}

#[async_trait::async_trait]
impl NotifyCancel for StateWatcher {
    async fn wait_until_cancelled(&self) -> anyhow::Result<()> {
        let mut state = self.clone();
        while !state.is_cancelled() {
            state.changed().await?;
        }

        Ok(())
    }

    fn is_cancelled(&self) -> bool {
        let state = self.borrow();
        state.stopping() || state.stopped()
    }
}

/// A token that implements [`NotifyCancel`]. Given to jobs inside of [`TaskManager`] so they can
/// stop either when commanded by the [`TaskManager`] or by an outside source.
#[derive(Clone)]
pub struct CancellationToken {
    outside_signal: Arc<dyn NotifyCancel + Send + Sync>,
    inner_signal: tokio_util::sync::CancellationToken,
}

impl CancellationToken {
    pub fn new(outside_signal: impl NotifyCancel + Send + Sync + 'static) -> Self {
        Self {
            outside_signal: Arc::new(outside_signal),
            inner_signal: tokio_util::sync::CancellationToken::new(),
        }
    }

    pub fn cancel(&self) {
        self.inner_signal.cancel()
    }
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.inner_signal.is_cancelled() || self.outside_signal.is_cancelled()
    }
}

impl<T> TaskManager<T> {
    pub fn new(outside_cancel: impl NotifyCancel + Send + Sync + 'static) -> Self {
        Self {
            set: JoinSet::new(),
            cancel_token: CancellationToken::new(outside_cancel),
        }
    }

    pub fn run<F>(&mut self, arg: F) -> anyhow::Result<T>
    where
        F: FnOnce(CancellationToken) -> anyhow::Result<T>,
    {
        arg(self.cancel_token.clone())
    }
}

impl<T> TaskManager<T>
where
    T: Send + 'static,
{
    #[cfg(test)]
    pub fn spawn<F, Fut>(&mut self, arg: F)
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: futures::Future<Output = anyhow::Result<T>> + Send + 'static,
    {
        let token = self.cancel_token.clone();
        self.set.spawn(arg(token));
    }

    pub fn spawn_blocking<F>(&mut self, arg: F)
    where
        F: FnOnce(CancellationToken) -> anyhow::Result<T> + Send + 'static,
    {
        let token = self.cancel_token.clone();
        self.set.spawn_blocking(move || arg(token));
    }

    pub async fn wait(self) -> anyhow::Result<Vec<T>> {
        let results = futures::stream::unfold(self.set, |mut set| async move {
            let res = set.join_next().await?;
            Some((res, set))
        })
        .map(|result| result.map_err(Into::into).and_then(|r| r))
        .inspect_err(|_| self.cancel_token.cancel())
        .collect::<Vec<_>>()
        .await;

        results.into_iter().try_collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use anyhow::bail;
    use tokio_util::sync::CancellationToken as TokioCancelToken;

    use crate::service::genesis::task_manager::{
        NotifyCancel,
        TaskManager,
    };

    #[tokio::test]
    async fn task_added_and_completed() {
        // given
        let mut workers = TaskManager::new(TokioCancelToken::new());
        workers.spawn_blocking(|_| Ok(8u8));

        // when
        let results = workers.wait().await.unwrap();

        // then
        assert_eq!(results, vec![8]);
    }

    #[tokio::test]
    async fn returns_err_on_single_failure() {
        // given
        let mut workers = TaskManager::new(TokioCancelToken::new());
        workers.spawn_blocking(|_| Ok(10u8));
        workers.spawn_blocking(|_| Err(anyhow::anyhow!("I fail")));

        // when
        let results = workers.wait().await;

        // then
        let err = results.unwrap_err();
        assert_eq!(err.to_string(), "I fail");
    }

    #[tokio::test]
    async fn signals_cancel_to_non_finished_tasks_on_failure() {
        // given
        let mut workers = TaskManager::new(TokioCancelToken::new());
        let (tx, rx) = tokio::sync::oneshot::channel();
        workers.spawn(move |token| async move {
            token.inner_signal.wait_until_cancelled().await.unwrap();
            tx.send(()).unwrap();
            Ok(())
        });

        // when
        workers.spawn_blocking(|_| bail!("I fail"));

        // then
        let _ = workers.wait().await;
        tokio::time::timeout(Duration::from_secs(2), rx)
            .await
            .expect("Cancellation should have been signaled")
            .unwrap();
    }

    #[tokio::test]
    async fn stops_on_cancellation() {
        // given
        let cancel = TokioCancelToken::new();
        let mut workers = TaskManager::new(cancel.clone());

        workers.spawn(move |token| async move {
            token.outside_signal.wait_until_cancelled().await.unwrap();
            Ok(10u8)
        });

        // when
        cancel.cancel();

        // then
        let result = tokio::time::timeout(Duration::from_secs(2), workers.wait())
            .await
            .expect("Cancellation should have been signaled")
            .unwrap();

        assert_eq!(result, vec![10]);
    }
}
