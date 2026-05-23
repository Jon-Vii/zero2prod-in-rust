use axum::{Router, http::StatusCode, routing::get};
use tokio::net::TcpListener;

async fn health_check() -> StatusCode {
    StatusCode::OK
}

pub fn app() -> Router {
    Router::new().route("/health_check", get(health_check))
}

pub async fn run(listener: TcpListener) -> Result<(), std::io::Error> {
    axum::serve(listener, app()).await
}

pub async fn run_on(address: &str) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(address).await?;
    run(listener).await
}
