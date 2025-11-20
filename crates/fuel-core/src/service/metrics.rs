use axum::{
    body::Body,
    http::Request,
    response::{IntoResponse, Response},
};
use fuel_core_metrics::encode_metrics;

pub fn encode_metrics_response() -> impl IntoResponse {
    let encoded = encode_metrics();
    if let Ok(body) = encoded {
        Response::builder()
            .status(200)
            .body(Body::from(body))
            .unwrap()
    } else {
        error_body()
    }
}

fn error_body() -> Response<Body> {
    Response::builder()
        .status(503)
        .body(Body::from(""))
        .unwrap()
}

pub async fn metrics(_req: Request<Body>) -> impl IntoResponse {
    encode_metrics_response()
}
