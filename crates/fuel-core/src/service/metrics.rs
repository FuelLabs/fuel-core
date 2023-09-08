use axum::{
    body::Body,
    http::Request,
    response::IntoResponse,
};
use fuel_core_metrics::response::encode_metrics_response;

pub async fn metrics(_req: Request<Body>) -> impl IntoResponse {
    encode_metrics_response()
}
