use crate::{
    client::from_strings_errors_to_std_error,
    reqwest_ext::{
        FuelGraphQlResponse,
        FuelOperation,
        ReqwestExt,
    },
};
use async_trait::async_trait;
use base64::Engine;
#[cfg(feature = "subscriptions")]
use base64::prelude::BASE64_STANDARD;
use cynic::{
    Operation,
    QueryFragment,
    QueryVariables,
    StreamingOperation,
    SubscriptionBuilder,
};
use fuel_core_types::fuel_types::BlockHeight;
use futures::{
    Stream,
    StreamExt,
};
use reqwest::Url;
use serde::{
    Serialize,
    de::DeserializeOwned,
};
use std::{
    fmt::Debug,
    io,
    pin::Pin,
    sync::Arc,
};

#[async_trait]
pub trait Transport: Debug + Send + Sync + 'static {
    async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<FuelGraphQlResponse<ResponseData>>
    where
        Vars: serde::Serialize + QueryVariables + Clone + Send + 'static,
        ResponseData: DeserializeOwned + QueryFragment + Send + 'static;

    #[cfg(feature = "subscriptions")]
    async fn subscribe<'a, ResponseData, Variables>(
        &self,
        variables: Variables,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<Pin<Box<dyn Stream<Item = io::Result<ResponseData>> + Send + 'a>>>
    where
        Variables: Serialize + QueryVariables + Send + Clone + 'static,
        ResponseData: DeserializeOwned
            + QueryFragment
            + Send
            + 'static
            + SubscriptionBuilder<Variables>;

    fn decode_response<R, E>(response: FuelGraphQlResponse<R, E>) -> io::Result<R>
    where
        R: serde::de::DeserializeOwned + 'static,
    {
        if response
            .extensions
            .as_ref()
            .and_then(|e| e.fuel_block_height_precondition_failed)
            == Some(true)
        {
            return Err(io::Error::other("The required block height was not met"));
        }

        let response = response.response;

        match (response.data, response.errors) {
            (Some(d), _) => Ok(d),
            (_, Some(e)) => Err(from_strings_errors_to_std_error(
                e.into_iter().map(|e| e.message).collect(),
            )),
            _ => Err(io::Error::other("Invalid response")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FailoverTransport {
    client: reqwest::Client,
    urls: Box<[Url]>,
    #[cfg(feature = "subscriptions")]
    cookie: Arc<reqwest::cookie::Jar>,
}

impl FailoverTransport {
    pub fn new(
        urls: Vec<Url>,
        cookie: Arc<reqwest::cookie::Jar>,
    ) -> reqwest::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .cookie_provider(cookie.clone())
                .build()?,
            urls: urls.into(),
            cookie,
        })
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
            .post(url.clone())
            .run_fuel_graphql(fuel_operation)
            .await
            .map_err(io::Error::other)
    }

    #[tracing::instrument(skip_all)]
    #[cfg(feature = "subscriptions")]
    async fn internal_subscribe<'a, ResponseData, Vars>(
        &self,
        q: StreamingOperation<ResponseData, Vars>,
        url: Url,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<Pin<Box<dyn Stream<Item = io::Result<ResponseData>> + Send + 'a>>>
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

        let mut last = None;

        enum Event<ResponseData> {
            Connected,
            ResponseData(ResponseData),
        }

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
                                match Self::decode_response(resp) {
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
                                    Err(e) => Some(Err(io::Error::other(format!(
                                        "Decode error: {e:?}"
                                    )))),
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

        Ok(Box::pin(stream))
    }
}

#[async_trait]
impl Transport for FailoverTransport {
    async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<FuelGraphQlResponse<ResponseData>>
    where
        Vars: Serialize + QueryVariables + Send + Clone + 'static,
        ResponseData: DeserializeOwned + QueryFragment + Send + 'static,
    {
        let mut last_err = None;

        for url in self.urls.iter() {
            let query = clone_operation(&q);
            match self
                .internal_query(query, url.clone(), required_block_height)
                .await
            {
                Ok(response_data) => return Ok(response_data),
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err.unwrap())
    }

    #[cfg(feature = "subscriptions")]
    async fn subscribe<'a, ResponseData, Variables>(
        &self,
        variables: Variables,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<Pin<Box<dyn Stream<Item = io::Result<ResponseData>> + Send + 'a>>>
    where
        Variables: Serialize + QueryVariables + Send + Clone + 'static,
        ResponseData: DeserializeOwned
            + QueryFragment
            + Send
            + 'static
            + SubscriptionBuilder<Variables>,
    {
        let mut last_err = None;

        for url in self.urls.iter() {
            let query = ResponseData::build(variables.clone());
            match self
                .internal_subscribe(query, url.clone(), required_block_height)
                .await
            {
                Ok(response_data) => return Ok(response_data),
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err.unwrap())
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
