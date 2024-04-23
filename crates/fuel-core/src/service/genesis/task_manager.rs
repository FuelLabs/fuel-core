use std::sync::Arc;

use fuel_core_services::StateWatcher;
use futures::{
    stream::FuturesUnordered,
    StreamExt,
    TryStreamExt,
};
use itertools::Itertools;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub struct TaskManager<T> {
    set: JoinSet<anyhow::Result<T>>,
    cancel_listener: MultiCancellationToken,
    cancel_tasks: CancellationToken,
}

#[async_trait::async_trait]
pub trait NotifyCancel {
    async fn wait_until_cancelled(&self) -> anyhow::Result<()>;
    fn is_cancelled(&self) -> bool;
}

#[async_trait::async_trait]
impl NotifyCancel for CancellationToken {
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

/// A token that can be used to monitor multiple cancellation sources.
#[derive(Default, Clone)]
pub struct MultiCancellationToken {
    sources: Vec<Arc<dyn NotifyCancel + Send + Sync>>,
}

impl MultiCancellationToken {
    pub fn from_single(source: impl NotifyCancel + Send + Sync + 'static) -> Self {
        let mut token = Self::default();
        token.insert(source);
        token
    }

    /// Note: Adding a new source to the token will not affect already running futures.
    pub fn insert(&mut self, source: impl NotifyCancel + Send + Sync + 'static) {
        self.sources.push(Arc::new(source));
    }
}

#[async_trait::async_trait]
impl NotifyCancel for MultiCancellationToken {
    async fn wait_until_cancelled(&self) -> anyhow::Result<()> {
        if self.sources.is_empty() {
            futures::future::pending::<()>().await;
            Ok(())
        } else {
            FuturesUnordered::from_iter(
                self.sources.iter().map(|s| s.wait_until_cancelled()),
            )
            .next()
            .await
            .expect("At least one source should be available")
        }
    }

    fn is_cancelled(&self) -> bool {
        self.sources.iter().any(|s| s.is_cancelled())
    }
}

impl<T> Default for TaskManager<T> {
    fn default() -> Self {
        Self::new(CancellationToken::new())
    }
}

impl<T> TaskManager<T> {
    pub fn new(cancel_token: impl NotifyCancel + Send + Sync + 'static) -> Self {
        let task_cancel = CancellationToken::new();
        let mut multi_cancel = MultiCancellationToken::from_single(cancel_token);
        multi_cancel.insert(task_cancel.child_token());
        Self {
            set: JoinSet::new(),
            cancel_listener: multi_cancel,
            cancel_tasks: task_cancel,
        }
    }
}

impl<T> TaskManager<T>
where
    T: Send + 'static,
{
    #[cfg(test)]
    pub fn spawn<F, Fut>(&mut self, arg: F)
    where
        F: FnOnce(MultiCancellationToken) -> Fut,
        Fut: futures::Future<Output = anyhow::Result<T>> + Send + 'static,
    {
        let token = self.cancel_listener.clone();
        self.set.spawn(arg(token));
    }

    pub fn spawn_blocking<F>(&mut self, arg: F)
    where
        F: FnOnce(MultiCancellationToken) -> anyhow::Result<T> + Send + 'static,
    {
        let token = self.cancel_listener.clone();
        self.set.spawn_blocking(move || arg(token));
    }

    pub async fn wait(self) -> anyhow::Result<Vec<T>> {
        let results = futures::stream::unfold(self.set, |mut set| async move {
            let res = set.join_next().await?;
            Some((res, set))
        })
        .map(|result| result.map_err(Into::into).and_then(|r| r))
        .inspect_err(|_| self.cancel_tasks.cancel())
        .collect::<Vec<_>>()
        .await;

        results.into_iter().try_collect()
    }
}

#[cfg(test)]
mod tests {
    mod cancel_token {
        use std::time::Duration;

        use tokio::sync::watch;
        use tokio_util::sync::CancellationToken;

        use crate::service::genesis::task_manager::{
            MultiCancellationToken,
            NotifyCancel,
        };

        #[tokio::test]
        async fn reacts_on_tokio_token_being_cancelled() {
            // given
            let mut multi_token = MultiCancellationToken::default();

            let dud_token = CancellationToken::new();
            multi_token.insert(dud_token);

            let active_token = CancellationToken::new();
            multi_token.insert(active_token.clone());

            // when
            active_token.cancel();

            // then
            tokio::time::timeout(
                Duration::from_secs(1),
                multi_token.wait_until_cancelled(),
            )
            .await
            .expect("Cancel future should have resolved")
            .unwrap();
            assert!(multi_token.is_cancelled());
        }

        #[tokio::test]
        async fn reacts_on_state_watcher_stopping() {
            // given
            use fuel_core_services::StateWatcher;
            let mut multi_token = MultiCancellationToken::default();

            let dud_token = CancellationToken::new();
            multi_token.insert(dud_token);

            let (tx, rx) = watch::channel(fuel_core_services::State::NotStarted);
            let active_token: StateWatcher = rx.into();
            multi_token.insert(active_token);

            // when
            tx.send(fuel_core_services::State::Stopping).unwrap();

            // then
            assert!(multi_token.is_cancelled());
            tokio::time::timeout(
                Duration::from_secs(1),
                multi_token.wait_until_cancelled(),
            )
            .await
            .expect("Cancel future should have resolved")
            .unwrap();
        }
    }
    mod state_watcher {
        use std::time::Duration;

        use anyhow::bail;
        use tokio_util::sync::CancellationToken;

        use crate::service::genesis::task_manager::{
            NotifyCancel,
            TaskManager,
        };

        #[tokio::test]
        async fn task_added_and_completed() {
            // given
            let mut workers = TaskManager::default();
            workers.spawn_blocking(|_| Ok(8u8));

            // when
            let results = workers.wait().await.unwrap();

            // then
            assert_eq!(results, vec![8]);
        }

        #[tokio::test]
        async fn returns_err_on_single_failure() {
            // given
            let mut workers = TaskManager::default();
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
            let mut workers = TaskManager::default();
            let (tx, rx) = tokio::sync::oneshot::channel();
            workers.spawn(move |token| async move {
                token.wait_until_cancelled().await.unwrap();
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
            let cancel = CancellationToken::new();
            let mut workers = TaskManager::new(cancel.clone());

            workers.spawn(move |token| async move {
                token.wait_until_cancelled().await.unwrap();
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
}
