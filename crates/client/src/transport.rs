use crate::reqwest_ext::{
    FuelGraphQlResponse,
    FuelOperation,
    ReqwestExt,
};
use cynic::{
    Operation,
    QueryFragment,
    QueryVariables,
};
#[cfg(feature = "subscriptions")]
use cynic::{
    StreamingOperation,
    SubscriptionBuilder,
};
use fuel_core_types::fuel_types::BlockHeight;
#[cfg(feature = "subscriptions")]
use futures::StreamExt;
use reqwest::Url;
use serde::{
    Serialize,
    de::DeserializeOwned,
};
#[cfg(feature = "subscriptions")]
use std::sync::Arc;
use std::{
    fmt::Debug,
    io,
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

#[derive(Debug)]
pub struct FailoverTransport {
    client: reqwest::Client,
    urls: Box<[Url]>,
    default_url_index: AtomicUsize,
    #[cfg(feature = "subscriptions")]
    cookie: Arc<reqwest::cookie::Jar>,
}

impl Clone for FailoverTransport {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            urls: self.urls.clone(),
            default_url_index: AtomicUsize::new(
                self.default_url_index.load(Ordering::Relaxed),
            ),
            #[cfg(feature = "subscriptions")]
            cookie: self.cookie.clone(),
        }
    }
}

impl FailoverTransport {
    pub fn new(urls: Vec<Url>) -> reqwest::Result<Self> {
        #[cfg(feature = "subscriptions")]
        {
            let cookie = std::sync::Arc::new(reqwest::cookie::Jar::default());
            let client = Self::client_builder()
                .cookie_provider(cookie.clone())
                .build()?;
            Ok(Self {
                urls: urls.into_boxed_slice(),
                client,
                default_url_index: AtomicUsize::new(0),
                cookie,
            })
        }

        #[cfg(not(feature = "subscriptions"))]
        {
            let client = Self::client_builder().build()?;
            Ok(Self {
                client,
                urls: urls.into_boxed_slice(),
                default_url_index: AtomicUsize::new(0),
            })
        }
    }

    /// All transport (queries AND status subscriptions) goes through one
    /// `reqwest` client speaking HTTP/2 with prior knowledge, so every
    /// concurrent request/stream multiplexes over a single connection per
    /// node instead of opening one TCP connection per subscription (which
    /// exhausts client ephemeral ports at a few thousand submissions per
    /// second). fuel-core's GraphQL server (hyper) auto-detects the h2
    /// preface on cleartext connections. For a proxy that cannot speak
    /// h2c, set `FUEL_GRAPHQL_HTTP1=1` to fall back to HTTP/1.1.
    fn client_builder() -> reqwest::ClientBuilder {
        let builder = reqwest::Client::builder();
        if std::env::var_os("FUEL_GRAPHQL_HTTP1").is_some() {
            builder
        } else {
            builder.http2_prior_knowledge()
        }
    }

    pub fn get_default_url(&self) -> &Url {
        let default_index = self.default_url_index.load(Ordering::Relaxed);
        &self.urls[default_index]
    }

    pub async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<FuelGraphQlResponse<ResponseData>>
    where
        Vars: Serialize + QueryVariables + Clone + Send + 'static,
        ResponseData: DeserializeOwned + QueryFragment + Send + 'static,
    {
        let mut last_err = None;
        let urls_count = self.urls.len();
        if urls_count == 0 {
            return Err(io::Error::other(
                "Failover transport has no URLs configured",
            ));
        }

        let default_index = self.default_url_index.load(Ordering::Relaxed);
        for url_offset in 0..urls_count {
            let url_index = default_index
                .saturating_add(url_offset)
                .checked_rem(urls_count)
                .ok_or_else(|| io::Error::other("Invalid URL count"))?;
            let url = self.urls[url_index].clone();
            let query = clone_operation(&q);
            match self.internal_query(query, url, required_block_height).await {
                Ok(response_data) => {
                    if url_offset != 0 {
                        self.default_url_index.store(url_index, Ordering::Relaxed);
                    }
                    return Ok(response_data);
                }
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err.unwrap())
    }

    #[cfg(feature = "subscriptions")]
    pub async fn subscribe<ResponseData, Variables>(
        &self,
        variables: Variables,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<
        impl futures::Stream<Item = io::Result<FuelGraphQlResponse<ResponseData>>> + '_,
    >
    where
        Variables: Serialize + QueryVariables + Send + Clone + 'static,
        ResponseData: DeserializeOwned
            + QueryFragment
            + Send
            + 'static
            + SubscriptionBuilder<Variables>,
    {
        let mut last_err = None;

        let urls_count = self.urls.len();
        if urls_count == 0 {
            return Err(io::Error::other(
                "Failover transport has no URLs configured",
            ));
        }

        let default_index = self.default_url_index.load(Ordering::Relaxed);

        for url_offset in 0..urls_count {
            let url_index = default_index
                .saturating_add(url_offset)
                .checked_rem(urls_count)
                .ok_or_else(|| io::Error::other("Invalid URL count"))?;
            let url = self.urls[url_index].clone();
            let query = ResponseData::build(variables.clone());
            match self
                .internal_subscribe(query, url, required_block_height)
                .await
            {
                Ok(response_data) => {
                    if url_offset != 0 {
                        self.default_url_index.store(url_index, Ordering::Relaxed);
                    }
                    return Ok(response_data);
                }
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err.unwrap())
    }

    async fn internal_query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
        url: Url,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<FuelGraphQlResponse<ResponseData>>
    where
        Vars: serde::Serialize + QueryVariables + Send + Clone + 'static,
        ResponseData: DeserializeOwned + QueryFragment + Send + 'static,
    {
        let fuel_operation = FuelOperation::new(q, required_block_height);
        self.client
            .post(url)
            .run_fuel_graphql(fuel_operation)
            .await
            .map_err(io::Error::other)
    }

    #[tracing::instrument(skip_all)]
    #[cfg(feature = "subscriptions")]
    async fn internal_subscribe<ResponseData, Vars>(
        &self,
        q: StreamingOperation<ResponseData, Vars>,
        url: Url,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<
        impl futures::Stream<Item = io::Result<FuelGraphQlResponse<ResponseData>>> + '_,
    >
    where
        Vars: serde::Serialize,
        ResponseData: serde::de::DeserializeOwned + 'static + Send,
    {
        let mut url = url.clone();
        url.set_path("/v1/graphql-sub");

        let fuel_operation = FuelOperation::new(q, required_block_height);
        let json_query = serde_json::to_string(&fuel_operation)?;

        let mut request = self
            .client
            .post(url.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .body(json_query);
        if let Some(password) = url.password() {
            request = request.basic_auth(url.username(), Some(password));
        }

        // An accepted response is the historical `Connected` event: connection
        // errors surface here, before a stream is returned.
        let response = request
            .send()
            .await
            .map_err(|e| io::Error::other(format!("Graphql error: {e:?}")))?
            .error_for_status()
            .map_err(|e| io::Error::other(format!("Graphql error: {e:?}")))?;

        let stream = futures::stream::try_unfold(
            SseState {
                bytes: response.bytes_stream().boxed(),
                buffer: Vec::new(),
                pending: std::collections::VecDeque::new(),
                last: None,
                done: false,
            },
            |mut state| async move {
                loop {
                    if let Some(data) = state.pending.pop_front() {
                        // Remove duplicates (same payload delivered twice in a
                        // row), mirroring the historical eventsource path.
                        if state.last.as_deref() == Some(data.as_str()) {
                            continue;
                        }
                        let resp = serde_json::from_str::<
                            FuelGraphQlResponse<ResponseData>,
                        >(&data)
                        .map_err(|e| io::Error::other(format!("Json error: {e:?}")))?;
                        state.last = Some(data);
                        return Ok(Some((resp, state)));
                    }
                    if state.done {
                        return Ok(None);
                    }
                    match state.bytes.next().await {
                        Some(Ok(chunk)) => {
                            state.buffer.extend_from_slice(&chunk);
                            let events = drain_sse_events(&mut state.buffer);
                            state.pending.extend(events);
                        }
                        Some(Err(e)) => {
                            return Err(io::Error::other(format!("Graphql error: {e:?}")));
                        }
                        None => {
                            state.done = true;
                        }
                    }
                }
            },
        );

        Ok(stream)
    }
}

/// Streaming-parse state for one server-sent-events subscription.
#[cfg(feature = "subscriptions")]
struct SseState {
    bytes: futures::stream::BoxStream<'static, reqwest::Result<bytes::Bytes>>,
    buffer: Vec<u8>,
    pending: std::collections::VecDeque<String>,
    last: Option<String>,
    done: bool,
}

/// Extract the `data` payloads of all COMPLETE server-sent events from the
/// accumulator (events are terminated by a blank line). Comment/`event:`
/// lines are ignored; multi-line data is joined with newlines per the SSE
/// specification. Incomplete trailing bytes stay in the buffer.
#[cfg(feature = "subscriptions")]
fn drain_sse_events(buffer: &mut Vec<u8>) -> Vec<String> {
    let mut out = Vec::new();
    loop {
        // An event ends at the first blank line: "\n\n" (or CRLF variants).
        let mut frame_end = None;
        let mut i = 0usize;
        while let Some(window_start) = i.checked_add(1) {
            if window_start >= buffer.len() {
                break;
            }
            let (a, b) = (buffer[i], buffer[window_start]);
            if a == b'\n' && b == b'\n' {
                frame_end = Some((i, i.saturating_add(2)));
                break;
            }
            if a == b'\n' && b == b'\r' && buffer.get(i.saturating_add(2)) == Some(&b'\n')
            {
                frame_end = Some((i, i.saturating_add(3)));
                break;
            }
            i = window_start;
        }
        let Some((data_end, consumed)) = frame_end else {
            break;
        };
        let frame: Vec<u8> = buffer.drain(..consumed).collect();
        let text = String::from_utf8_lossy(&frame[..data_end]);
        let mut data_lines: Vec<&str> = Vec::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        if !data_lines.is_empty() {
            out.push(data_lines.join("\n"));
        }
    }
    out
}

fn clone_operation<ResponseData, Vars>(
    op: &Operation<ResponseData, Vars>,
) -> Operation<ResponseData, Vars>
where
    Vars: QueryVariables + Clone,
    ResponseData: QueryFragment,
{
    let mut cloned = Operation::new(op.query.clone(), op.variables.clone());
    cloned.operation_name = op.operation_name.clone();
    cloned
}
