pub mod configuration;

use axum::{
    Router,
    extract::{Form, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::net::TcpListener;
use uuid::Uuid;

async fn health_check() -> StatusCode {
    StatusCode::OK
}

#[derive(Deserialize)]
struct SubscribeForm {
    name: String,
    email: String,
}

async fn subscribe(State(pool): State<PgPool>, Form(form): Form<SubscribeForm>) -> StatusCode {
    match sqlx::query(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, now())
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(form.email)
    .bind(form.name)
    .execute(&pool)
    .await
    {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/health_check", get(health_check))
        .route("/subscriptions", post(subscribe))
        .with_state(pool)
}

pub async fn run(listener: TcpListener, pool: PgPool) -> Result<(), std::io::Error> {
    axum::serve(listener, app(pool)).await
}

pub async fn run_on(address: &str, pool: PgPool) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(address).await?;
    run(listener, pool).await
}
