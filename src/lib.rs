pub mod configuration;
pub mod domain;
pub mod email_client;
mod routes;
pub mod telemetry;

use crate::email_client::EmailClient;
use axum::{
    Router,
    http::{HeaderName, Request},
    routing::{get, post},
};
use routes::{confirm, health_check, publish_newsletter, subscribe};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct ApplicationState {
    pub db_pool: PgPool,
    pub email_client: EmailClient,
    pub application_base_url: String,
}

pub fn app(state: ApplicationState) -> Router {
    let request_id_header = HeaderName::from_static("x-request-id");
    let request_id_header_for_span = request_id_header.clone();

    Router::new()
        .route("/health_check", get(health_check))
        .route("/subscriptions", post(subscribe))
        .route("/subscriptions/confirm", get(confirm))
        .route("/newsletters", post(publish_newsletter))
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
        .with_state(state)
}

pub async fn run(listener: TcpListener, state: ApplicationState) -> Result<(), std::io::Error> {
    axum::serve(listener, app(state)).await
}

pub async fn run_on(address: &str, state: ApplicationState) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(address).await?;
    run(listener, state).await
}
