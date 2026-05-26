pub mod configuration;
pub mod domain;
mod routes;
pub mod telemetry;

use axum::{
    Router,
    http::{HeaderName, Request},
    routing::{get, post},
};
use routes::{health_check, subscribe};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

pub fn app(pool: PgPool) -> Router {
    let request_id_header = HeaderName::from_static("x-request-id");
    let request_id_header_for_span = request_id_header.clone();

    Router::new()
        .route("/health_check", get(health_check))
        .route("/subscriptions", post(subscribe))
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(
            TraceLayer::new_for_http().make_span_with(move |request: &Request<_>| {
                let request_id = request
                    .headers()
                    .get(&request_id_header_for_span)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("unknown");

                tracing::info_span!(
                    "http-request",
                    method = %request.method(),
                    uri = %request.uri(),
                    request_id = %request_id,
                )
            }),
        )
        .layer(SetRequestIdLayer::new(request_id_header, MakeRequestUuid))
        .with_state(pool)
}

pub async fn run(listener: TcpListener, pool: PgPool) -> Result<(), std::io::Error> {
    axum::serve(listener, app(pool)).await
}

pub async fn run_on(address: &str, pool: PgPool) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(address).await?;
    run(listener, pool).await
}
