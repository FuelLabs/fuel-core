use async_graphql::{
    Response,
    ServerError,
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextExecute,
    },
};
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::sync::Semaphore;

pub struct ExpensiveOpGuardFactory {
    expensive_op_names: Arc<[String]>,
    semaphore: Arc<Semaphore>,
    timeout: Duration,
}

impl ExpensiveOpGuardFactory {
    pub fn new(
        expensive_op_names: Arc<[String]>,
        max_in_flight: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            expensive_op_names,
            semaphore: Arc::new(Semaphore::new(max_in_flight)),
            timeout,
        }
    }
}

impl ExtensionFactory for ExpensiveOpGuardFactory {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(ExpensiveOpGuard {
            expensive_op_names: self.expensive_op_names.clone(),
            semaphore: self.semaphore.clone(),
            timeout: self.timeout,
        })
    }
}

pub struct ExpensiveOpGuard {
    expensive_op_names: Arc<[String]>,
    semaphore: Arc<Semaphore>,
    timeout: Duration,
}

#[async_trait::async_trait]
impl Extension for ExpensiveOpGuard {
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let op = operation_name.clone().unwrap_or_default();
        let is_expensive = self.expensive_op_names.iter().any(|name| op == name);

        tracing::debug!(
            "Executing operation: {:?}, and expected one of {:?}, expensive: {:?}, timeout: {:?}, semaphore_size: {:?}",
            operation_name,
            self.expensive_op_names,
            is_expensive,
            self.timeout,
            self.semaphore.available_permits(),
        );

        if !is_expensive {
            return next.run(ctx, operation_name).await;
        }

        // Concurrency gate (bulkhead)
        let permit = match self.semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                let mut resp = Response::new(async_graphql::Value::Null);
                resp.errors.push(ServerError::new(
                    "Rate limit exceeded for this operation",
                    None,
                ));
                return resp;
            }
        };

        // Time bound (avoid request pile-ups)
        let fut = next.run(ctx, operation_name);
        let starting_time = tokio::time::Instant::now();
        let out = tokio::time::timeout(self.timeout, fut).await;
        tracing::warn!(
            "finished executing in {:?}ns, success: {:?}",
            starting_time.elapsed().as_nanos(),
            out.is_ok(),
        );

        drop(permit);

        match out {
            Ok(resp) => resp,
            Err(_) => {
                let mut resp = Response::new(async_graphql::Value::Null);
                resp.errors
                    .push(ServerError::new("Operation timed out", None));
                resp
            }
        }
    }
}
