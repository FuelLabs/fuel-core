use std::future::Future;
use std::pin::Pin;
use std::{fmt, sync::Arc};

use tonic::{
    codegen::Service,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
};

use crate::endpoint::{EndpointMetrics, RequestLabels};

#[derive(Clone)]
pub struct AuthInterceptor {
    pub token: Option<MetadataValue<Ascii>>,
}

impl std::fmt::Debug for AuthInterceptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.token {
            Some(_) => f.write_str("token_redacted"),
            None => f.write_str("no_token_configured"),
        }
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(ref t) = self.token {
            req.metadata_mut().insert("authorization", t.clone());
        }

        Ok(req)
    }
}

pub struct MetricsInterceptor<S> {
    pub(crate) metrics: Arc<EndpointMetrics>,
    pub(crate) service: S,
    pub(crate) labels: RequestLabels,
}

impl<S, Request> Service<Request> for MetricsInterceptor<S>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    Request: fmt::Debug,
{
    type Response = S::Response;

    type Error = S::Error;

    type Future = Pin<Box<dyn Future<Output = <S::Future as Future>::Output> + Send + 'static>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let labels = self.labels.clone();
        let metrics = self.metrics.clone();

        let fut = self.service.call(req);
        let res = async move {
            let res = fut.await;
            if res.is_ok() {
                metrics.success(&labels);
            } else {
                metrics.failure(&labels);
            }
            res
        };

        Box::pin(res)
    }
}
