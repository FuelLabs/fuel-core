use crate::quorum::{
    Quorum,
    QuorumError,
};
use alloy_json_rpc::{
    RequestPacket,
    Response,
    ResponsePacket,
    ResponsePayload,
    RpcError,
};
use alloy_transport::{
    BoxTransport,
    TransportError,
    TransportErrorKind,
};
use futures::FutureExt;
use std::{
    pin::Pin,
    task::Poll,
};
use tower::Service;

#[derive(Clone)]
pub struct QuorumTransport {
    transports: Vec<WeightedTransport>,
    quorum_weight: u64,
}

impl QuorumTransport {
    pub fn builder() -> QuorumTransportBuilder {
        QuorumTransportBuilder::default()
    }
}

impl Service<RequestPacket> for QuorumTransport {
    type Response = ResponsePacket;
    type Error = RpcError<TransportErrorKind>;
    type Future = Pin<
        Box<
            dyn Future<Output = Result<ResponsePacket, RpcError<TransportErrorKind>>>
                + Send
                + 'static,
        >,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let requests = self
            .transports
            .iter()
            .enumerate()
            .map(|(id, transport)| PendingRequest {
                future: transport.inner.clone().call(req.clone()),
                id,
            })
            .collect::<Vec<_>>();

        let quorum_request = QuorumRequest::new(self.clone(), requests);

        Box::pin(quorum_request)
    }
}

#[derive(Debug, Clone)]
pub struct WeightedTransport {
    inner: BoxTransport,
    pub weight: u64,
}

impl WeightedTransport {
    pub fn new(inner: BoxTransport) -> Self {
        Self { inner, weight: 1 }
    }

    pub fn with_weight(inner: BoxTransport, weight: u64) -> Self {
        Self { inner, weight }
    }
}

#[derive(Debug, Clone, Default)]
pub struct QuorumTransportBuilder {
    quorum: Quorum,
    transports: Vec<WeightedTransport>,
}

impl QuorumTransportBuilder {
    pub fn add_transports(
        mut self,
        providers: impl IntoIterator<Item = WeightedTransport>,
    ) -> Self {
        for provider in providers {
            self.transports.push(provider);
        }
        self
    }

    /// Set the kind of quorum
    pub fn quorum(mut self, quorum: Quorum) -> Self {
        self.quorum = quorum;
        self
    }

    pub fn build(self) -> QuorumTransport {
        let quorum_weight = self.quorum.weight(&self.transports);
        QuorumTransport {
            transports: self.transports,
            quorum_weight,
        }
    }
}

pub struct QuorumRequest<'a> {
    inner: QuorumTransport,
    /// The different answers with their cumulative weight
    responses: Vec<(ResponsePacket, u64)>,
    /// All the errors the provider yielded
    errors: Vec<TransportError>,
    // Requests currently pending
    requests: Vec<PendingRequest<'a>>,
}

type PendingRequestFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ResponsePacket, RpcError<TransportErrorKind>>>
            + 'a
            + Send,
    >,
>;

struct PendingRequest<'a> {
    future: PendingRequestFuture<'a>,
    id: usize,
}

impl Future for PendingRequest<'_> {
    type Output = Result<ResponsePacket, RpcError<TransportErrorKind>>;
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        self.future.as_mut().poll(cx)
    }
}

impl<'a> QuorumRequest<'a> {
    fn new(inner: QuorumTransport, requests: Vec<PendingRequest<'a>>) -> Self {
        Self {
            responses: Vec::new(),
            errors: Vec::new(),
            inner,
            requests,
        }
    }
}

impl<'a> Future for QuorumRequest<'a> {
    type Output = Result<ResponsePacket, RpcError<TransportErrorKind>>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        for n in (0..this.requests.len()).rev() {
            let mut request = this.requests.swap_remove(n);
            match request.poll_unpin(cx) {
                Poll::Ready(Ok(val)) => {
                    let response_weight = this.inner.transports[request.id].weight;
                    if let Some((_, weight)) = this
                        .responses
                        .iter_mut()
                        .find(|(v, _)| compare_response_packets(&val, v))
                    {
                        // add the weight to equal response value
                        *weight += response_weight;
                        if *weight >= this.inner.quorum_weight {
                            // reached quorum with multiple responses
                            return Poll::Ready(Ok(val));
                        } else {
                            this.responses.push((val, response_weight));
                        }
                    } else if response_weight >= this.inner.quorum_weight {
                        // reached quorum with single response
                        return Poll::Ready(Ok(val));
                    } else {
                        this.responses.push((val, response_weight));
                    }
                }
                Poll::Ready(Err(err)) => this.errors.push(err),
                _ => {
                    this.requests.push(request);
                }
            }
        }

        if this.requests.is_empty() {
            // No more requests and no quorum reached
            this.responses.sort_by(|a, b| b.1.cmp(&a.1));
            let values = std::mem::take(&mut this.responses)
                .into_iter()
                .map(|r| r.0)
                .collect();
            let errors = std::mem::take(&mut this.errors);
            Poll::Ready(Err(TransportErrorKind::custom(Box::new(
                QuorumError::NoQuorumReached { values, errors },
            ))))
        } else {
            Poll::Pending
        }
    }
}

fn compare_responses(first: &Response, second: &Response) -> bool {
    if (first.id != second.id) {
        return false;
    }
    match (&first.payload, &second.payload) {
        (ResponsePayload::Success(first), ResponsePayload::Success(second)) => {
            first.get() == second.get()
        }
        (ResponsePayload::Failure(first), ResponsePayload::Failure(second)) => {
            first.code == second.code
        }
        _ => false,
    }
}

fn compare_response_packets(a: &ResponsePacket, b: &ResponsePacket) -> bool {
    match (a, b) {
        (ResponsePacket::Single(first), ResponsePacket::Single(second)) => {
            compare_responses(first, second)
        }
        (ResponsePacket::Batch(first), ResponsePacket::Batch(second)) => first
            .iter()
            .zip(second)
            .all(|(a, b)| compare_responses(a, b)),
        _ => false,
    }
}
