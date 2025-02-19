use cynic::{
    http::CynicReqwestError,
    GraphQlResponse,
    Operation,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtensionsRequest {
    pub required_fuel_block_height: Option<BlockHeight>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExtensionsResponse {
    pub required_fuel_block_height: Option<BlockHeight>,
    pub current_fuel_block_height: Option<BlockHeight>,
    pub fuel_block_height_precondition_failed: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct FuelOperation<Operation> {
    #[serde(flatten)]
    pub operation: Operation,
    pub extensions: ExtensionsRequest,
}

#[derive(Debug, serde::Deserialize)]
pub struct FuelGraphQlResponse<T, ErrorExtensions = serde::de::IgnoredAny> {
    #[serde(flatten)]
    pub response: GraphQlResponse<T, ErrorExtensions>,
    pub extensions: Option<ExtensionsResponse>,
}

impl<Operation> FuelOperation<Operation> {
    pub fn new(
        operation: Operation,
        required_fuel_block_height: Option<BlockHeight>,
    ) -> Self {
        Self {
            operation,
            extensions: ExtensionsRequest {
                required_fuel_block_height,
            },
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[cfg(target_arch = "wasm32")]
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// An extension trait for reqwest::RequestBuilder.
///
/// ```rust,no_run
/// # mod schema {
/// #   cynic::use_schema!("../schemas/starwars.schema.graphql");
/// # }
/// #
/// # #[derive(cynic::QueryFragment)]
/// # #[cynic(
/// #    schema_path = "../schemas/starwars.schema.graphql",
/// #    schema_module = "schema",
/// # )]
/// # struct Film {
/// #    title: Option<String>,
/// #    director: Option<String>
/// # }
/// #
/// # #[derive(cynic::QueryFragment)]
/// # #[cynic(
/// #     schema_path = "../schemas/starwars.schema.graphql",
/// #     schema_module = "schema",
/// #     graphql_type = "Root"
/// # )]
/// # struct FilmDirectorQuery {
/// #     #[arguments(id = cynic::Id::new("ZmlsbXM6MQ=="))]
/// #     film: Option<Film>,
/// # }
/// use cynic::{http::ReqwestExt, QueryBuilder};
///
/// # async move {
/// let operation = FilmDirectorQuery::build(());
///
/// let client = reqwest::Client::new();
/// let response = client.post("https://swapi-graphql.netlify.app/.netlify/functions/index")
///     .run_graphql(operation)
///     .await
///     .unwrap();
///
/// println!(
///     "The director is {}",
///     response.data
///         .and_then(|d| d.film)
///         .and_then(|f| f.director)
///         .unwrap()
/// );
/// # };
/// ```
pub trait ReqwestExt {
    /// Runs a GraphQL query with the parameters in RequestBuilder, deserializes
    /// the and returns the result.
    fn run_fuel_graphql<ResponseData, Vars>(
        self,
        operation: FuelOperation<Operation<ResponseData, Vars>>,
    ) -> CynicReqwestBuilder<ResponseData>
    where
        Vars: serde::Serialize,
        ResponseData: serde::de::DeserializeOwned + 'static;
}

/// A builder for cynics reqwest integration
///
/// Implements `IntoFuture`, users should `.await` the builder or call
/// `into_future` directly when they're ready to send the request.
pub struct CynicReqwestBuilder<ResponseData, ErrorExtensions = serde::de::IgnoredAny> {
    builder: reqwest::RequestBuilder,
    _marker: std::marker::PhantomData<fn() -> (ResponseData, ErrorExtensions)>,
}

impl<ResponseData, Errors> CynicReqwestBuilder<ResponseData, Errors> {
    pub fn new(builder: reqwest::RequestBuilder) -> Self {
        Self {
            builder,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<ResponseData: serde::de::DeserializeOwned, Errors: serde::de::DeserializeOwned>
    std::future::IntoFuture for CynicReqwestBuilder<ResponseData, Errors>
{
    type Output = Result<FuelGraphQlResponse<ResponseData, Errors>, CynicReqwestError>;

    type IntoFuture = BoxFuture<
        'static,
        Result<FuelGraphQlResponse<ResponseData, Errors>, CynicReqwestError>,
    >;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let http_result = self.builder.send().await;
            deser_gql(http_result).await
        })
    }
}

impl<ResponseData> CynicReqwestBuilder<ResponseData, serde::de::IgnoredAny> {
    /// Sets the type that will be deserialized for the extensions fields of any errors in the response
    pub fn retain_extensions<ErrorExtensions>(
        self,
    ) -> CynicReqwestBuilder<ResponseData, ErrorExtensions>
    where
        ErrorExtensions: serde::de::DeserializeOwned,
    {
        let CynicReqwestBuilder { builder, _marker } = self;

        CynicReqwestBuilder {
            builder,
            _marker: PhantomData,
        }
    }
}

async fn deser_gql<ResponseData, ErrorExtensions>(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<FuelGraphQlResponse<ResponseData, ErrorExtensions>, CynicReqwestError>
where
    ResponseData: serde::de::DeserializeOwned,
    ErrorExtensions: serde::de::DeserializeOwned,
{
    let response = match response {
        Ok(response) => response,
        Err(e) => return Err(CynicReqwestError::ReqwestError(e)),
    };

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await;
        let text = match text {
            Ok(text) => text,
            Err(e) => return Err(CynicReqwestError::ReqwestError(e)),
        };

        let Ok(deserred) = serde_json::from_str(&text) else {
            let response = CynicReqwestError::ErrorResponse(status, text);
            return Err(response);
        };

        Ok(deserred)
    } else {
        let json = response.json().await;
        json.map_err(CynicReqwestError::ReqwestError)
    }
}

impl ReqwestExt for reqwest::RequestBuilder {
    fn run_fuel_graphql<ResponseData, Vars>(
        self,
        operation: FuelOperation<Operation<ResponseData, Vars>>,
    ) -> CynicReqwestBuilder<ResponseData>
    where
        Vars: serde::Serialize,
        ResponseData: serde::de::DeserializeOwned + 'static,
    {
        CynicReqwestBuilder::new(self.json(&operation))
    }
}
