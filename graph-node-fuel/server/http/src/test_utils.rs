use graph::prelude::serde_json;
use graph::prelude::*;
use http::StatusCode;
use hyper::{header::ACCESS_CONTROL_ALLOW_ORIGIN, Body, Response};

/// Asserts that the response is a successful GraphQL response; returns its `"data"` field.
pub fn assert_successful_response(
    response: Response<Body>,
) -> serde_json::Map<String, serde_json::Value> {
    assert_expected_headers(&response);
    futures03::executor::block_on(
        hyper::body::to_bytes(response.into_body())
            .map_ok(|chunk| {
                let json: serde_json::Value =
                    serde_json::from_slice(&chunk).expect("GraphQL response is not valid JSON");

                json.as_object()
                    .expect("GraphQL response must be an object")
                    .get("data")
                    .expect("GraphQL response must contain a \"data\" field")
                    .as_object()
                    .expect("GraphQL \"data\" field must be an object")
                    .clone()
            })
            .map_err(|e| panic!("Truncated response body {:?}", e)),
    )
    .unwrap()
}

/// Asserts that the response is a failed GraphQL response; returns its `"errors"` field.
pub fn assert_error_response(
    response: Response<Body>,
    expected_status: StatusCode,
    graphql_response: bool,
) -> Vec<serde_json::Value> {
    assert_eq!(response.status(), expected_status);
    assert_expected_headers(&response);
    let body = String::from_utf8(
        futures03::executor::block_on(hyper::body::to_bytes(response.into_body()))
            .unwrap()
            .to_vec(),
    )
    .unwrap();

    // In case of a non-graphql response, return the body.
    if !graphql_response {
        return vec![serde_json::Value::String(body)];
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).expect("GraphQL response is not valid JSON");

    json.as_object()
        .expect("GraphQL response must be an object")
        .get("errors")
        .expect("GraphQL error response must contain an \"errors\" field")
        .as_array()
        .expect("GraphQL \"errors\" field must be a vector")
        .clone()
}

#[track_caller]
pub fn assert_expected_headers(response: &Response<Body>) {
    assert_eq!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .expect("Missing CORS Header"),
        &"*"
    );
}
