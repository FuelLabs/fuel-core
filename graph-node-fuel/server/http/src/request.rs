use graph::prelude::serde_json;
use hyper::body::Bytes;

use graph::components::server::query::GraphQLServerError;
use graph::prelude::*;

pub fn parse_graphql_request(body: &Bytes, trace: bool) -> Result<Query, GraphQLServerError> {
    // Parse request body as JSON
    let json: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| GraphQLServerError::ClientError(format!("{}", e)))?;

    // Ensure the JSON data is an object
    let obj = json.as_object().ok_or_else(|| {
        GraphQLServerError::ClientError(String::from("Request data is not an object"))
    })?;

    // Ensure the JSON data has a "query" field
    let query_value = obj.get("query").ok_or_else(|| {
        GraphQLServerError::ClientError(String::from(
            "The \"query\" field is missing in request data",
        ))
    })?;

    // Ensure the "query" field is a string
    let query_string = query_value.as_str().ok_or_else(|| {
        GraphQLServerError::ClientError(String::from("The \"query\" field is not a string"))
    })?;

    // Parse the "query" field of the JSON body
    let document = graphql_parser::parse_query(query_string)
        .map_err(|e| GraphQLServerError::from(QueryError::ParseError(Arc::new(e.into()))))?
        .into_static();

    // Parse the "variables" field of the JSON body, if present
    let variables = match obj.get("variables") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(variables @ serde_json::Value::Object(_)) => serde_json::from_value(variables.clone())
            .map_err(|e| GraphQLServerError::ClientError(e.to_string()))
            .map(Some),
        _ => Err(GraphQLServerError::ClientError(
            "Invalid query variables provided".to_string(),
        )),
    }?;

    Ok(Query::new(document, variables, trace))
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use graph::{
        data::{
            query::QueryTarget,
            value::{Object, Word},
        },
        prelude::*,
    };

    use super::parse_graphql_request;

    lazy_static! {
        static ref TARGET: QueryTarget = QueryTarget::Name(
            SubgraphName::new("test/request").unwrap(),
            Default::default()
        );
    }

    #[test]
    fn rejects_invalid_json() {
        let request = parse_graphql_request(&hyper::body::Bytes::from("!@#)%"), false);
        request.expect_err("Should reject invalid JSON");
    }

    #[test]
    fn rejects_json_without_query_field() {
        let request = parse_graphql_request(&hyper::body::Bytes::from("{}"), false);
        request.expect_err("Should reject JSON without query field");
    }

    #[test]
    fn rejects_json_with_non_string_query_field() {
        let request = parse_graphql_request(&hyper::body::Bytes::from("{\"query\": 5}"), false);
        request.expect_err("Should reject JSON with a non-string query field");
    }

    #[test]
    fn rejects_broken_queries() {
        let request =
            parse_graphql_request(&hyper::body::Bytes::from("{\"query\": \"foo\"}"), false);
        request.expect_err("Should reject broken queries");
    }

    #[test]
    fn accepts_valid_queries() {
        let request = parse_graphql_request(
            &hyper::body::Bytes::from("{\"query\": \"{ user { name } }\"}"),
            false,
        );
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
        let request = parse_graphql_request(
            &hyper::body::Bytes::from(
                "\
                 {\
                 \"query\": \"{ user { name } }\", \
                 \"variables\": null \
                 }",
            ),
            false,
        );
        let query = request.expect("Should accept null variables");

        let expected_query = graphql_parser::parse_query("{ user { name } }")
            .unwrap()
            .into_static();
        assert_eq!(query.document, expected_query);
        assert_eq!(query.variables, None);
    }

    #[test]
    fn rejects_non_map_variables() {
        let request = parse_graphql_request(
            &hyper::body::Bytes::from(
                "\
                 {\
                 \"query\": \"{ user { name } }\", \
                 \"variables\": 5 \
                 }",
            ),
            false,
        );
        request.expect_err("Should reject non-map variables");
    }

    #[test]
    fn parses_variables() {
        let request = parse_graphql_request(
            &hyper::body::Bytes::from(
                "\
                 {\
                 \"query\": \"{ user { name } }\", \
                 \"variables\": { \
                 \"string\": \"s\", \"map\": {\"k\": \"v\"}, \"int\": 5 \
                 } \
                 }",
            ),
            false,
        );
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
