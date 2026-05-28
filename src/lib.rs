pub mod authentication;
pub mod configuration;
pub mod domain;
pub mod email_client;
pub mod idempotency;
pub mod issue_delivery_worker;
mod routes;
pub mod session_state;
pub mod telemetry;

use crate::email_client::EmailClient;
use axum::{
    Router,
    http::{HeaderName, Request},
    routing::{get, post},
};
use routes::{
    admin_dashboard, admin_publish_newsletter, change_password_form, change_password_handler,
    confirm, health_check, login, login_form, logout, publish_newsletter_form, subscribe,
};
use secrecy::ExposeSecret;
use sqlx::PgPool;
use time::Duration;
use tokio::net::TcpListener;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tower_sessions::{
    Expiry, SessionManagerLayer, SessionStore,
    cookie::{Key, SameSite},
};
use tower_sessions_redis_store::{RedisStore, fred::prelude::*};

#[derive(Clone)]
pub struct ApplicationState {
    pub db_pool: PgPool,
    pub email_client: EmailClient,
    pub application_base_url: String,
}

pub fn app<Store>(state: ApplicationState, session_store: Store, hmac_secret: &[u8]) -> Router
where
    Store: SessionStore + Clone + Send + Sync + 'static,
{
    let request_id_header = HeaderName::from_static("x-request-id");
    let request_id_header_for_span = request_id_header.clone();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_http_only(true)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::hours(1)))
        .with_signed(Key::derive_from(hmac_secret));

    Router::new()
        .route("/health_check", get(health_check))
        .route("/subscriptions", post(subscribe))
        .route("/subscriptions/confirm", get(confirm))
        .route("/login", get(login_form).post(login))
        .route("/admin/dashboard", get(admin_dashboard))
        .route(
            "/admin/newsletters",
            get(publish_newsletter_form).post(admin_publish_newsletter),
        )
        .route(
            "/admin/password",
            get(change_password_form).post(change_password_handler),
        )
        .route("/admin/logout", post(logout))
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
        .layer(session_layer)
        .with_state(state)
}

pub async fn build_redis_store(
    redis_uri: &secrecy::SecretString,
) -> Result<RedisStore<Pool>, Box<dyn std::error::Error>> {
    let config = Config::from_url(redis_uri.expose_secret())?;
    let pool = Pool::new(config, None, None, None, 6)?;
    let redis_connection = pool.connect();
    pool.wait_for_connect().await?;
    tokio::spawn(async move {
        if let Err(error) = redis_connection.await {
            tracing::error!(%error, "Redis connection task failed");
        }
    });
    Ok(RedisStore::new(pool))
}

pub async fn run<Store>(
    listener: TcpListener,
    state: ApplicationState,
    session_store: Store,
    hmac_secret: &[u8],
) -> Result<(), std::io::Error>
where
    Store: SessionStore + Clone + Send + Sync + 'static,
{
    axum::serve(listener, app(state, session_store, hmac_secret)).await
}

pub async fn run_on<Store>(
    address: &str,
    state: ApplicationState,
    session_store: Store,
    hmac_secret: &[u8],
) -> Result<(), std::io::Error>
where
    Store: SessionStore + Clone + Send + Sync + 'static,
{
    let listener = TcpListener::bind(address).await?;
    run(listener, state, session_store, hmac_secret).await
}
