use std::future::Future;

use fuel_core_services::StateWatcher;
use futures::{
    StreamExt,
    TryStreamExt,
};
use itertools::Itertools;
use tokio::task::JoinSet;

pub struct TaskManager<T> {
    set: JoinSet<anyhow::Result<T>>,
    cancel: CancellationToken,
}

#[cfg_attr(feature = "test-helpers", derive(Default))]
#[derive(Clone)]
pub struct CancellationToken {
    task_cancellator: tokio_util::sync::CancellationToken,
    state_watcher: StateWatcher,
}

#[cfg(feature = "test-helpers")]
impl From<tokio_util::sync::CancellationToken> for CancellationToken {
    fn from(token: tokio_util::sync::CancellationToken) -> Self {
        Self {
            task_cancellator: token,
            ..Default::default()
        }
    }
}

impl CancellationToken {
    #[cfg(test)]
    pub async fn cancelled(mut self) -> anyhow::Result<()> {
        tokio::select! {
            _ = self.task_cancellator.cancelled() => Ok(()),
            result = self.state_watcher.wait_stopping_or_stopped() => result
        }
    }

    fn cancel_tasks(&self) {
        self.task_cancellator.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        let state = self.state_watcher.borrow();
        self.task_cancellator.is_cancelled() || state.stopped() || state.stopping()
    }
}

impl<T> TaskManager<T>
where
    T: Send + 'static,
{
    pub fn new(watcher: StateWatcher) -> Self {
        Self {
            set: JoinSet::new(),
            cancel: CancellationToken {
                task_cancellator: tokio_util::sync::CancellationToken::new(),
                state_watcher: watcher,
            },
        }
    }

    pub fn spawn<F, Fut>(&mut self, arg: F)
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
    {
        self.set.spawn(arg(self.cancel.clone()));
    }

    pub async fn wait(self) -> anyhow::Result<Vec<T>> {
        let results = futures::stream::unfold(self.set, |mut set| async move {
            let res = set.join_next().await?;
            Some((res, set))
        })
        .map(|result| result.map_err(Into::into).and_then(|r| r))
        .inspect_err(|_| self.cancel.cancel_tasks())
        .collect::<Vec<_>>()
        .await;

        results.into_iter().try_collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use anyhow::bail;
    use fuel_core_services::State;
    use tokio::sync::watch;

    use super::*;

    #[tokio::test]
    async fn task_added_and_completed() {
        // given
        let mut workers = TaskManager::new(StateWatcher::default());
        workers.spawn(|_| async { Ok(8u8) });

        // when
        let results = workers.wait().await.unwrap();

        // then
        assert_eq!(results, vec![8]);
    }

    #[tokio::test]
    async fn returns_err_on_single_failure() {
        // given
        let mut workers = TaskManager::new(StateWatcher::default());
        workers.spawn(|_| async { Ok(10u8) });
        workers.spawn(|_| async { Err(anyhow::anyhow!("I fail")) });

        // when
        let results = workers.wait().await;

        // then
        let err = results.unwrap_err();
        assert_eq!(err.to_string(), "I fail");
    }

    #[tokio::test]
    async fn signals_cancel_to_non_finished_tasks_on_failure() {
        // given
        let (_sender, recv) = watch::channel(State::Started);
        let watcher = recv.into();
        let mut workers = TaskManager::new(watcher);
        let (tx, rx) = tokio::sync::oneshot::channel();
        workers.spawn(move |token| async move {
            token.cancelled().await.unwrap();
            tx.send(()).unwrap();
            Ok(())
        });

        // when
        workers.spawn(|_| async { bail!("I fail") });

        // then
        let _ = workers.wait().await;
        tokio::time::timeout(Duration::from_secs(2), rx)
            .await
            .expect("Cancellation should have been signaled")
            .unwrap();
    }

    #[tokio::test]
    async fn reacts_when_state_changes_to_stopping() {
        // given
        let (sender, receiver) = watch::channel(State::Started);
        let mut workers = TaskManager::new(receiver.into());

        workers.spawn(move |token| async move {
            token.cancelled().await.unwrap();
            Ok(10u8)
        });

        // when
        sender.send(State::Stopping).unwrap();

        // then
        let result = tokio::time::timeout(Duration::from_secs(2), workers.wait())
            .await
            .expect("Cancellation should have been signaled")
            .unwrap();

        assert_eq!(result, vec![10]);
    }
}
