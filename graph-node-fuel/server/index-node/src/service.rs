use graph::blockchain::BlockchainMap;
use http::header::{
    self, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    CONTENT_TYPE, LOCATION,
};
use hyper::body::Bytes;
use hyper::service::Service;
use hyper::{Body, Method, Request, Response, StatusCode};

use std::task::Context;
use std::task::Poll;

use graph::components::{server::query::GraphQLServerError, store::Store};
use graph::data::query::QueryResults;
use graph::prelude::*;
use graph_graphql::prelude::{execute_query, Query as PreparedQuery, QueryExecutionOptions};

use crate::auth::bearer_token;

use crate::explorer::Explorer;
use crate::resolver::IndexNodeResolver;
use crate::schema::SCHEMA;

struct NoopGraphQLMetrics;

impl GraphQLMetrics for NoopGraphQLMetrics {
    fn observe_query_execution(&self, _duration: Duration, _results: &QueryResults) {}
    fn observe_query_parsing(&self, _duration: Duration, _results: &QueryResults) {}
    fn observe_query_validation(&self, _duration: Duration, _id: &DeploymentHash) {}
    fn observe_query_validation_error(&self, _error_codes: Vec<&str>, _id: &DeploymentHash) {}
    fn observe_query_blocks_behind(&self, _blocks_behind: i32, _id: &DeploymentHash) {}
}

/// An asynchronous response to a GraphQL request.
pub type IndexNodeServiceResponse = DynTryFuture<'static, Response<Body>, GraphQLServerError>;

/// A Hyper Service that serves GraphQL over a POST / endpoint.
#[derive(Debug)]
pub struct IndexNodeService<Q, S> {
    logger: Logger,
    blockchain_map: Arc<BlockchainMap>,
    graphql_runner: Arc<Q>,
    store: Arc<S>,
    explorer: Arc<Explorer<S>>,
    link_resolver: Arc<dyn LinkResolver>,
}

impl<Q, S> Clone for IndexNodeService<Q, S> {
    fn clone(&self) -> Self {
        Self {
            logger: self.logger.clone(),
            blockchain_map: self.blockchain_map.clone(),
            graphql_runner: self.graphql_runner.clone(),
            store: self.store.clone(),
            explorer: self.explorer.clone(),
            link_resolver: self.link_resolver.clone(),
        }
    }
}

impl<Q, S> CheapClone for IndexNodeService<Q, S> {}

impl<Q, S> IndexNodeService<Q, S>
where
    Q: GraphQlRunner,
    S: Store,
{
    /// Creates a new GraphQL service.
    pub fn new(
        logger: Logger,
        blockchain_map: Arc<BlockchainMap>,
        graphql_runner: Arc<Q>,
        store: Arc<S>,
        link_resolver: Arc<dyn LinkResolver>,
    ) -> Self {
        let explorer = Arc::new(Explorer::new(store.clone()));

        IndexNodeService {
            logger,
            blockchain_map,
            graphql_runner,
            store,
            explorer,
            link_resolver,
        }
    }

    fn graphiql_html() -> &'static str {
        include_str!("../assets/index.html")
    }

    /// Serves a static file.
    fn serve_file(contents: &'static str, content_type: &'static str) -> Response<Body> {
        Response::builder()
            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(CONTENT_TYPE, content_type)
            .status(200)
            .body(Body::from(contents))
            .unwrap()
    }

    fn index() -> Response<Body> {
        Response::builder()
            .status(200)
            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(CONTENT_TYPE, "text/html")
            .body(Body::from("OK"))
            .unwrap()
    }

    fn handle_graphiql() -> Response<Body> {
        Self::serve_file(Self::graphiql_html(), "text/html")
    }

    pub async fn handle_graphql_query(
        &self,
        request: Request<Body>,
    ) -> Result<QueryResults, GraphQLServerError> {
        let (req_parts, req_body) = request.into_parts();
        let store = self.store.clone();

        // Obtain the schema for the index node GraphQL API
        let schema = SCHEMA.clone();

        let body = hyper::body::to_bytes(req_body)
            .map_err(|_| GraphQLServerError::InternalError("Failed to read request body".into()))
            .await?;

        let validated = ValidatedRequest::new(body, &req_parts.headers)?;
        let query = validated.query;

        let query = match PreparedQuery::new(
            &self.logger,
            schema,
            None,
            query,
            None,
            100,
            Arc::new(NoopGraphQLMetrics),
        ) {
            Ok(query) => query,
            Err(e) => return Ok(QueryResults::from(QueryResult::from(e))),
        };

        // Run the query using the index node resolver
        let query_clone = query.cheap_clone();
        let logger = self.logger.cheap_clone();
        let result: QueryResult = {
            let resolver = IndexNodeResolver::new(
                &logger,
                store,
                self.link_resolver.clone(),
                validated.bearer_token,
                self.blockchain_map.clone(),
            );
            let options = QueryExecutionOptions {
                resolver,
                deadline: None,
                max_first: std::u32::MAX,
                max_skip: std::u32::MAX,
                trace: false,
            };
            let result = execute_query(query_clone.cheap_clone(), None, None, options).await;
            query_clone.log_execution(0);
            // Index status queries are not cacheable, so we may unwrap this.
            Arc::try_unwrap(result).unwrap()
        };

        Ok(QueryResults::from(result))
    }

    // Handles OPTIONS requests
    fn handle_graphql_options(_request: Request<Body>) -> Response<Body> {
        Response::builder()
            .status(200)
            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(CONTENT_TYPE, "text/plain")
            .header(ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type, User-Agent")
            .header(ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS, POST")
            .body(Body::from(""))
            .unwrap()
    }

    /// Handles 302 redirects
    fn handle_temp_redirect(destination: &str) -> Result<Response<Body>, GraphQLServerError> {
        header::HeaderValue::from_str(destination)
            .map_err(|_| {
                GraphQLServerError::ClientError("invalid characters in redirect URL".into())
            })
            .map(|loc_header_val| {
                Response::builder()
                    .status(StatusCode::FOUND)
                    .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                    .header(CONTENT_TYPE, "text/plain")
                    .header(LOCATION, loc_header_val)
                    .body(Body::from("Redirecting..."))
                    .unwrap()
            })
    }

    /// Handles 404s.
    pub(crate) fn handle_not_found() -> Response<Body> {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(CONTENT_TYPE, "text/plain")
            .body(Body::from("Not found\n"))
            .unwrap()
    }

    async fn handle_call(self, req: Request<Body>) -> Result<Response<Body>, GraphQLServerError> {
        let method = req.method().clone();

        let path = req.uri().path().to_owned();
        let path_segments = {
            let mut segments = path.split('/');

            // Remove leading '/'
            assert_eq!(segments.next(), Some(""));

            segments.collect::<Vec<_>>()
        };

        match (method, path_segments.as_slice()) {
            (Method::GET, [""]) => Ok(Self::index()),
            (Method::GET, ["graphiql.css"]) => Ok(Self::serve_file(
                include_str!("../assets/graphiql.css"),
                "text/css",
            )),
            (Method::GET, ["graphiql.min.js"]) => Ok(Self::serve_file(
                include_str!("../assets/graphiql.min.js"),
                "text/javascript",
            )),

            (Method::GET, path @ ["graphql"]) => {
                let dest = format!("/{}/playground", path.join("/"));
                Self::handle_temp_redirect(&dest)
            }
            (Method::GET, ["graphql", "playground"]) => Ok(Self::handle_graphiql()),

            (Method::POST, ["graphql"]) => {
                Ok(self.handle_graphql_query(req).await?.as_http_response())
            }
            (Method::OPTIONS, ["graphql"]) => Ok(Self::handle_graphql_options(req)),

            (Method::GET, ["explorer", rest @ ..]) => self.explorer.handle(&self.logger, rest),

            _ => Ok(Self::handle_not_found()),
        }
    }
}

impl<Q, S> Service<Request<Body>> for IndexNodeService<Q, S>
where
    Q: GraphQlRunner,
    S: Store,
{
    type Response = Response<Body>;
    type Error = GraphQLServerError;
    type Future = IndexNodeServiceResponse;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let logger = self.logger.clone();

        // Returning Err here will prevent the client from receiving any response.
        // Instead, we generate a Response with an error code and return Ok
        Box::pin(
            self.cheap_clone()
                .handle_call(req)
                .map(move |result| match result {
                    Ok(response) => Ok(response),
                    Err(err @ GraphQLServerError::ClientError(_)) => {
                        debug!(logger, "IndexNodeService call failed: {}", err);

                        Ok(Response::builder()
                            .status(400)
                            .header(CONTENT_TYPE, "text/plain")
                            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .body(Body::from(format!("Invalid request: {}", err)))
                            .unwrap())
                    }
                    Err(err @ GraphQLServerError::QueryError(_)) => {
                        error!(logger, "IndexNodeService call failed: {}", err);

                        Ok(Response::builder()
                            .status(400)
                            .header(CONTENT_TYPE, "text/plain")
                            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .body(Body::from(format!("Query error: {}", err)))
                            .unwrap())
                    }
                    Err(err @ GraphQLServerError::InternalError(_)) => {
                        error!(logger, "IndexNodeService call failed: {}", err);

                        Ok(Response::builder()
                            .status(500)
                            .header(CONTENT_TYPE, "text/plain")
                            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .body(Body::from(format!("Internal server error: {}", err)))
                            .unwrap())
                    }
                }),
        )
    }
}

struct ValidatedRequest {
    pub query: Query,
    pub bearer_token: Option<String>,
}

impl ValidatedRequest {
    pub fn new(req_body: Bytes, headers: &hyper::HeaderMap) -> Result<Self, GraphQLServerError> {
        // Parse request body as JSON
        let json: serde_json::Value = serde_json::from_slice(&req_body)
            .map_err(|e| GraphQLServerError::ClientError(format!("{}", e)))?;

        // Ensure the JSON data is an object
        let obj = json.as_object().ok_or_else(|| {
            GraphQLServerError::ClientError(String::from("ValidatedRequest data is not an object"))
        })?;

        // Ensure the JSON data has a "query" field
        let query_value = obj.get("query").ok_or_else(|| {
            GraphQLServerError::ClientError(String::from(
                "The \"query\" field is missing in request data",
            ))
        })?;

        // Ensure the "query" field is a string
        let query_string = query_value.as_str().ok_or_else(|| {
            GraphQLServerError::ClientError(String::from("The\"query\" field is not a string"))
        })?;

        // Parse the "query" field of the JSON body
        let document = graphql_parser::parse_query(query_string)
            .map_err(|e| GraphQLServerError::from(QueryError::ParseError(Arc::new(e.into()))))?
            .into_static();

        // Parse the "variables" field of the JSON body, if present
        let variables = match obj.get("variables") {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(variables @ serde_json::Value::Object(_)) => {
                serde_json::from_value(variables.clone())
                    .map_err(|e| GraphQLServerError::ClientError(e.to_string()))
                    .map(Some)
            }
            _ => Err(GraphQLServerError::ClientError(
                "Invalid query variables provided".to_string(),
            )),
        }?;

        let query = Query::new(document, variables, false);
        let bearer_token = bearer_token(headers)
            .map(<[u8]>::to_vec)
            .map(String::from_utf8)
            .transpose()
            .map_err(|_| {
                GraphQLServerError::ClientError("Bearer token is invalid UTF-8".to_string())
            })?;

        Ok(Self {
            query,
            bearer_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use graph::{
        data::value::{Object, Word},
        prelude::*,
    };

    use hyper::body::Bytes;
    use hyper::HeaderMap;
    use std::collections::HashMap;

    use super::{GraphQLServerError, ValidatedRequest};

    fn validate_req(req_body: Bytes) -> Result<Query, GraphQLServerError> {
        Ok(ValidatedRequest::new(req_body, &HeaderMap::new())?.query)
    }

    #[test]
    fn rejects_invalid_json() {
        let request = validate_req(Bytes::from("!@#)%"));
        request.expect_err("Should reject invalid JSON");
    }

    #[test]
    fn rejects_json_without_query_field() {
        let request = validate_req(Bytes::from("{}"));
        request.expect_err("Should reject JSON without query field");
    }

    #[test]
    fn rejects_json_with_non_string_query_field() {
        let request = validate_req(Bytes::from("{\"query\": 5}"));
        request.expect_err("Should reject JSON with a non-string query field");
    }

    #[test]
    fn rejects_broken_queries() {
        let request = validate_req(Bytes::from("{\"query\": \"foo\"}"));
        request.expect_err("Should reject broken queries");
    }

    #[test]
    fn accepts_valid_queries() {
        let request = validate_req(Bytes::from("{\"query\": \"{ user { name } }\"}"));
        let query = request.expect("Should accept valid queries");
        assert_eq!(
            query.document,
            graphql_parser::parse_query("{ user { name } }")
                .unwrap()
                .into_static()
        );
    }

    #[test]
    fn accepts_null_variables() {
        let request = validate_req(Bytes::from(
            "\
                 {\
                 \"query\": \"{ user { name } }\", \
                 \"variables\": null \
                 }",
        ));
        let query = request.expect("Should accept null variables");

        let expected_query = graphql_parser::parse_query("{ user { name } }")
            .unwrap()
            .into_static();
        assert_eq!(query.document, expected_query);
        assert_eq!(query.variables, None);
    }

    #[test]
    fn rejects_non_map_variables() {
        let request = validate_req(Bytes::from(
            "\
                 {\
                 \"query\": \"{ user { name } }\", \
                 \"variables\": 5 \
                 }",
        ));
        request.expect_err("Should reject non-map variables");
    }

    #[test]
    fn parses_variables() {
        let request = validate_req(Bytes::from(
            "\
                 {\
                 \"query\": \"{ user { name } }\", \
                 \"variables\": { \
                 \"string\": \"s\", \"map\": {\"k\": \"v\"}, \"int\": 5 \
                 } \
                 }",
        ));
        let query = request.expect("Should accept valid queries");

        let expected_query = graphql_parser::parse_query("{ user { name } }")
            .unwrap()
            .into_static();
        let expected_variables = QueryVariables::new(HashMap::from_iter(
            vec![
                (String::from("string"), r::Value::String(String::from("s"))),
                (
                    String::from("map"),
                    r::Value::Object(Object::from_iter(
                        vec![(Word::from("k"), r::Value::String(String::from("v")))].into_iter(),
                    )),
                ),
                (String::from("int"), r::Value::Int(5)),
            ]
            .into_iter(),
        ));

        assert_eq!(query.document, expected_query);
        assert_eq!(query.variables, Some(expected_variables));
    }
}
