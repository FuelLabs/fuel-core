use crate::reqwest_ext::{
    FuelGraphQlResponse,
    FuelOperation,
    ReqwestExt,
};
#[cfg(feature = "subscriptions")]
use base64::prelude::{
    BASE64_STANDARD,
    Engine as _,
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
            let client = reqwest::Client::builder()
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
            let client = reqwest::Client::new();
            Ok(Self {
                client,
                urls: urls.into_boxed_slice(),
                default_url_index: AtomicUsize::new(0),
            })
        }
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
        use core::ops::Deref;
        use eventsource_client as es;
        use hyper_rustls as _;
        use reqwest::cookie::CookieStore;
        let mut url = url.clone();
        url.set_path("/v1/graphql-sub");

        let fuel_operation = FuelOperation::new(q, required_block_height);

        let json_query = serde_json::to_string(&fuel_operation)?;
        let mut client_builder = es::ClientBuilder::for_url(url.as_str())
            .map_err(|e| io::Error::other(format!("Failed to start client {e:?}")))?
            .body(json_query)
            .method("POST".to_string())
            .header("content-type", "application/json")
            .map_err(|e| {
                io::Error::other(format!("Failed to add header to client {e:?}"))
            })?;
        if let Some(password) = url.password() {
            let username = url.username();
            let credentials = format!("{}:{}", username, password);
            let authorization = format!("Basic {}", BASE64_STANDARD.encode(credentials));
            client_builder = client_builder
                .header("Authorization", &authorization)
                .map_err(|e| {
                    io::Error::other(format!("Failed to add header to client {e:?}"))
                })?;
        }

        if let Some(value) = self.cookie.deref().cookies(&url) {
            let value = value.to_str().map_err(|e| {
                io::Error::other(format!("Unable convert header value to string {e:?}"))
            })?;
            client_builder = client_builder
                .header(reqwest::header::COOKIE.as_str(), value)
                .map_err(|e| {
                    io::Error::other(format!(
                        "Failed to add header from `reqwest` to client {e:?}"
                    ))
                })?;
        }

        let client = client_builder.build_with_conn(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_webpki_roots()
                .https_or_http()
                .enable_http1()
                .build(),
        );

        enum Event<ResponseData> {
            Connected,
            ResponseData(ResponseData),
        }

        let mut last = None;

        let mut init_stream = es::Client::stream(&client)
            .take_while(|result| {
                futures::future::ready(!matches!(result, Err(es::Error::Eof)))
            })
            .filter_map(move |result| {
                tracing::debug!("Got result: {result:?}");
                let r = match result {
                    Ok(es::SSE::Event(es::Event { data, .. })) => {
                        match serde_json::from_str::<FuelGraphQlResponse<ResponseData>>(
                            &data,
                        ) {
                            Ok(resp) => {
                                match last.replace(data) {
                                    // Remove duplicates
                                    Some(l)
                                        if l == *last.as_ref().expect(
                                            "Safe because of the replace above",
                                        ) =>
                                    {
                                        None
                                    }
                                    _ => Some(Ok(Event::ResponseData(resp))),
                                }
                            }
                            Err(e) => {
                                Some(Err(io::Error::other(format!("Json error: {e:?}"))))
                            }
                        }
                    }
                    Ok(es::SSE::Connected(_)) => Some(Ok(Event::Connected)),
                    Ok(_) => None,
                    Err(e) => {
                        Some(Err(io::Error::other(format!("Graphql error: {e:?}"))))
                    }
                };
                futures::future::ready(r)
            });

        let event = init_stream.next().await;
        let stream_with_resp = init_stream.filter_map(|result| async move {
            match result {
                Ok(Event::Connected) => None,
                Ok(Event::ResponseData(resp)) => Some(Ok(resp)),
                Err(error) => Some(Err(error)),
            }
        });

        let stream = match event {
            Some(Ok(Event::Connected)) => {
                tracing::debug!("Subscription connected");
                stream_with_resp.boxed()
            }
            Some(Ok(Event::ResponseData(resp))) => {
                tracing::debug!("Subscription returned response");
                let joined_stream = futures::stream::once(async move { Ok(resp) })
                    .chain(stream_with_resp);
                joined_stream.boxed()
            }
            Some(Err(e)) => return Err(e),
            None => {
                return Err(io::Error::other("Subscription stream ended unexpectedly"));
            }
        };

        Ok(stream)
    }
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
