use std::future::Future;

use futures::{
    StreamExt,
    TryStreamExt,
};
use itertools::Itertools;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub struct TaskManager<T> {
    set: JoinSet<anyhow::Result<T>>,
    cancel_token: CancellationToken,
}

impl<T> TaskManager<T>
where
    T: Send + 'static,
{
    pub fn new(cancel_token: CancellationToken) -> Self {
        Self {
            set: JoinSet::new(),
            cancel_token,
        }
    }

    pub fn spawn<F, Fut>(&mut self, arg: F)
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
    {
        self.set.spawn(arg(self.cancel_token.clone()));
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

    use super::*;

    #[tokio::test]
    async fn task_added_and_completed() {
        // given
        let mut workers = TaskManager::new(CancellationToken::new());
        workers.spawn(|_| async { Ok(8u8) });

        // when
        let results = workers.wait().await.unwrap();

        // then
        assert_eq!(results, vec![8]);
    }

    #[tokio::test]
    async fn returns_err_on_single_failure() {
        // given
        let mut workers = TaskManager::new(CancellationToken::new());
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
        let mut workers = TaskManager::new(CancellationToken::new());
        let (tx, rx) = tokio::sync::oneshot::channel();
        workers.spawn(move |token| async move {
            token.cancelled().await;
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
    async fn propagates_cancellation_from_outside() {
        // given
        let cancel_token = CancellationToken::new();
        let mut workers = TaskManager::new(cancel_token.clone());

        workers.spawn(move |token| async move {
            token.cancelled().await;
            Ok(10u8)
        });

        // when
        cancel_token.cancel();

        // then
        let result = tokio::time::timeout(Duration::from_secs(2), workers.wait())
            .await
            .expect("Cancellation should have been signaled")
            .unwrap();

        assert_eq!(result, vec![10]);
    }
}
