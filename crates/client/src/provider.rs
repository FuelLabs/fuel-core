use crate::reqwest_ext::{
    FuelGraphQlResponse,
    FuelOperation,
    ReqwestExt,
};
use async_trait::async_trait;
use cynic::{
    Operation,
    QueryFragment,
    QueryVariables,
};
use fuel_core_types::fuel_types::BlockHeight;
use reqwest::Url;
use serde::de::DeserializeOwned;
use std::{
    fmt::Debug,
    io,
    sync::Arc,
};

#[async_trait]
pub trait Provider: Debug + Send + Sync + 'static {
    async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<FuelGraphQlResponse<ResponseData>>
    where
        Vars: serde::Serialize + QueryVariables + Clone + Send + 'static,
        ResponseData: DeserializeOwned + QueryFragment + Send + 'static;
}

#[derive(Debug, Clone)]
pub struct FailoverProvider {
    client: reqwest::Client,
    urls: Box<[Url]>,
}

impl FailoverProvider {
    pub fn new(
        urls: Vec<Url>,
        cookie: Arc<reqwest::cookie::Jar>,
    ) -> reqwest::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .cookie_provider(cookie.clone())
                .build()?,
            urls: urls.into(),
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
        Ok(self
            .client
            .post(url.clone())
            .run_fuel_graphql(fuel_operation)
            .await
            .map_err(io::Error::other)?)
    }
}

#[async_trait]
impl Provider for FailoverProvider {
    async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<FuelGraphQlResponse<ResponseData>>
    where
        Vars: serde::Serialize + QueryVariables + Send + Clone + 'static,
        ResponseData: DeserializeOwned + QueryFragment + Send + 'static,
    {
        let mut last_err = None;

        for url in self.urls.iter() {
            let query = clone_operation(&q);
            match self
                .internal_query(query, url.clone(), required_block_height.clone())
                .await
            {
                Ok(response_data) => return Ok(response_data),
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err.unwrap())
    }
}

#[derive(Debug)]
pub struct BaseProvider(FailoverProvider);

impl BaseProvider {
    pub fn new(url: Url, cookie: Arc<reqwest::cookie::Jar>) -> reqwest::Result<Self> {
        Ok(Self(FailoverProvider::new(vec![url], cookie)?))
    }
}

#[async_trait]
impl Provider for BaseProvider {
    async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
        required_block_height: Option<BlockHeight>,
    ) -> io::Result<FuelGraphQlResponse<ResponseData>>
    where
        Vars: serde::Serialize + QueryVariables + Clone + Send + 'static,
        ResponseData: DeserializeOwned + QueryFragment + Send + 'static,
    {
        self.0.query(q, required_block_height).await
    }
}

fn clone_operation<ResponseData, Vars>(
    op: &Operation<ResponseData, Vars>,
) -> Operation<ResponseData, Vars>
where
    Vars: QueryVariables + Clone,
    ResponseData: QueryFragment,
{
    let mut copy = Operation::new(op.query.clone(), op.variables.clone());
    copy.operation_name = op.operation_name.clone();
    copy
}
