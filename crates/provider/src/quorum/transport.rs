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
use parking_lot::Mutex;
use std::{
    pin::Pin,
    sync::Arc,
    task::Poll,
};
use tower::Service;

#[derive(Clone)]
pub struct QuorumTransport {
    transports: Arc<[WeightedTransport]>,
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
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let requests = self
            .transports
            .iter()
            .enumerate()
            .map(|(id, transport)| PendingRequest {
                future: transport.inner.lock().call(req.clone()),
                id,
            })
            .collect::<Vec<_>>();

        let quorum_request = QuorumRequest::new(self.clone(), requests);

        Box::pin(quorum_request)
    }
}

#[derive(Debug)]
pub struct WeightedTransport {
    inner: Mutex<BoxTransport>,
    pub weight: u64,
}

impl WeightedTransport {
    pub fn new(inner: BoxTransport) -> Self {
        Self {
            inner: Mutex::new(inner),
            weight: 1,
        }
    }

    pub fn with_weight(inner: BoxTransport, weight: u64) -> Self {
        Self {
            inner: Mutex::new(inner),
            weight,
        }
    }
}

#[derive(Debug, Default)]
pub struct QuorumTransportBuilder {
    quorum: Quorum,
    transports: Vec<WeightedTransport>,
}

impl QuorumTransportBuilder {
    pub fn with_transports(
        mut self,
        providers: impl IntoIterator<Item = WeightedTransport>,
    ) -> Self {
        for provider in providers {
            self.transports.push(provider);
        }
        self
    }

    /// Set the kind of quorum
    pub fn with_quorum(mut self, quorum: Quorum) -> Self {
        self.quorum = quorum;
        self
    }

    pub fn build(self) -> QuorumTransport {
        let quorum_weight = self.quorum.weight(&self.transports);
        QuorumTransport {
            transports: self.transports.into_boxed_slice().into(),
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
        let num_transports = inner.transports.len();
        Self {
            responses: Vec::with_capacity(num_transports),
            errors: Vec::with_capacity(num_transports),
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
            let values: Vec<_> = std::mem::take(&mut this.responses)
                .into_iter()
                .map(|r| r.0)
                .collect();
            let errors = std::mem::take(&mut this.errors);
            Poll::Ready(Err(TransportErrorKind::custom(Box::new(
                QuorumError::NoQuorumReached {
                    values: values.into(),
                    errors: errors.into(),
                },
            ))))
        } else {
            Poll::Pending
        }
    }
}

fn compare_responses(a: &Response, b: &Response) -> bool {
    if a.id != b.id {
        return false;
    }

    match (&a.payload, &b.payload) {
        (ResponsePayload::Success(a), ResponsePayload::Success(b)) => {
            // Semantic JSON comparison, ignores key ordering and formatting differences
            match (
                serde_json::from_str::<serde_json::Value>(a.get()),
                serde_json::from_str::<serde_json::Value>(b.get()),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => a.get() == b.get(),
            }
        }
        (ResponsePayload::Failure(a), ResponsePayload::Failure(b)) => a.code == b.code,
        _ => false,
    }
}

fn compare_response_packets(a: &ResponsePacket, b: &ResponsePacket) -> bool {
    match (a, b) {
        (ResponsePacket::Single(a), ResponsePacket::Single(b)) => compare_responses(a, b),
        (ResponsePacket::Batch(a), ResponsePacket::Batch(b)) => {
            a.iter().zip(b).all(|(x, y)| compare_responses(x, y))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_json_rpc::{
        Id,
        Response,
        ResponsePacket,
        ResponsePayload,
    };
    use alloy_transport::{
        IntoBoxTransport,
        mock::{
            Asserter,
            MockTransport,
        },
    };
    use serde_json::value::RawValue;

    fn success_packet(raw_json: &str) -> ResponsePacket {
        ResponsePacket::Single(Response {
            id: Id::Number(1),
            payload: ResponsePayload::Success(
                RawValue::from_string(raw_json.to_string()).unwrap(),
            ),
        })
    }

    fn dummy_weighted_transport() -> WeightedTransport {
        WeightedTransport::new(MockTransport::new(Asserter::new()).into_box_transport())
    }

    /// Builds a QuorumRequest with two providers returning the given JSON payloads.
    /// quorum_weight = 2 means both providers must agree, so both responses are always
    /// compared before quorum can be reached or declared missing.
    fn make_quorum_request(
        json_a: &'static str,
        json_b: &'static str,
    ) -> QuorumRequest<'static> {
        let transports: Arc<[WeightedTransport]> =
            vec![dummy_weighted_transport(), dummy_weighted_transport()].into();

        let quorum_transport = QuorumTransport {
            transports,
            quorum_weight: 2,
        };

        QuorumRequest {
            inner: quorum_transport,
            responses: Vec::new(),
            errors: Vec::new(),
            requests: vec![
                PendingRequest {
                    future: Box::pin(std::future::ready(Ok(success_packet(json_a)))),
                    id: 0,
                },
                PendingRequest {
                    future: Box::pin(std::future::ready(Ok(success_packet(json_b)))),
                    id: 1,
                },
            ],
        }
    }

    // --- Unit tests: compare_response_packets ---

    #[test]
    fn identical_json_is_equal() {
        let a = success_packet(r#"{"address":"0xabc","data":"0x01"}"#);
        let b = success_packet(r#"{"address":"0xabc","data":"0x01"}"#);
        assert!(compare_response_packets(&a, &b));
    }

    #[test]
    fn different_key_order_is_equal() {
        let a = success_packet(r#"{"address":"0xabc","data":"0x01","logIndex":"0x0"}"#);
        let b = success_packet(r#"{"logIndex":"0x0","data":"0x01","address":"0xabc"}"#);
        assert!(compare_response_packets(&a, &b));
    }

    #[test]
    fn different_whitespace_is_equal() {
        let a = success_packet(r#"{"a":1,"b":2}"#);
        let b = success_packet(r#"{ "a" : 1 , "b" : 2 }"#);
        assert!(compare_response_packets(&a, &b));
    }

    #[test]
    fn genuinely_different_values_not_equal() {
        let a = success_packet(r#"{"address":"0xabc"}"#);
        let b = success_packet(r#"{"address":"0xdef"}"#);
        assert!(!compare_response_packets(&a, &b));
    }

    // --- Integration tests: QuorumRequest resolution ---

    #[tokio::test]
    async fn quorum_reached_with_different_json_key_ordering() {
        // Reproduces the production failure: two QuickNode backends return the same
        // eth_getLogs data but with different JSON field ordering (e.g. Geth vs Reth
        // serialization). Before the fix, string comparison caused NoQuorumReached.
        let json_a = r#"{"address":"0xdeadbeef","blockHash":"0x1234","data":"0xabcd","logIndex":"0x0"}"#;
        let json_b = r#"{"logIndex":"0x0","data":"0xabcd","blockHash":"0x1234","address":"0xdeadbeef"}"#;

        let result = make_quorum_request(json_a, json_b).await;
        assert!(
            result.is_ok(),
            "quorum must be reached when providers return identical data with different key ordering"
        );
    }

    #[tokio::test]
    async fn quorum_not_reached_for_genuinely_different_values() {
        let json_a = r#"{"address":"0xabc","data":"0x01"}"#;
        let json_b = r#"{"address":"0xdef","data":"0x02"}"#;

        let result = make_quorum_request(json_a, json_b).await;
        assert!(
            result.is_err(),
            "quorum must not be reached when providers return genuinely different values"
        );
    }
}
