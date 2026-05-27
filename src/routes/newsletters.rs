use crate::{ApplicationState, domain::SubscriberEmail};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::Row;
use thiserror::Error;

#[derive(Deserialize)]
pub struct Newsletter {
    title: String,
    text_content: String,
    html_content: String,
}

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("invalid newsletter issue")]
    ValidationError,
    #[error("failed to fetch confirmed subscribers")]
    SubscriberLookupError(#[source] sqlx::Error),
    #[error("invalid subscriber email stored in the database")]
    InvalidSubscriberEmail,
    #[error("failed to send newsletter issue")]
    SendEmailError(#[source] reqwest::Error),
}

impl IntoResponse for PublishError {
    fn into_response(self) -> Response {
        match self {
            Self::ValidationError => StatusCode::BAD_REQUEST.into_response(),
            Self::SubscriberLookupError(error) => {
                tracing::error!(%error, "Failed to fetch confirmed subscribers");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::InvalidSubscriberEmail => {
                tracing::error!("Invalid subscriber email stored in the database");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::SendEmailError(error) => {
                tracing::error!(%error, "Failed to send newsletter issue");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

pub async fn publish_newsletter(
    State(state): State<ApplicationState>,
    newsletter: Result<Json<Newsletter>, JsonRejection>,
) -> Result<StatusCode, PublishError> {
    let Json(newsletter) = newsletter.map_err(|_| PublishError::ValidationError)?;

    let confirmed_subscribers = get_confirmed_subscribers(&state.db_pool).await?;

    for subscriber in confirmed_subscribers {
        state
            .email_client
            .send_email(
                subscriber.email,
                &newsletter.title,
                &newsletter.html_content,
                &newsletter.text_content,
            )
            .await
            .map_err(PublishError::SendEmailError)?;
    }

    Ok(StatusCode::OK)
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

async fn get_confirmed_subscribers(
    pool: &sqlx::PgPool,
) -> Result<Vec<ConfirmedSubscriber>, PublishError> {
    let rows = sqlx::query(
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(PublishError::SubscriberLookupError)?;

    rows.into_iter()
        .map(|row| {
            let email: String = row.get("email");
            SubscriberEmail::parse(email)
                .map(|email| ConfirmedSubscriber { email })
                .map_err(|_| PublishError::InvalidSubscriberEmail)
        })
        .collect()
}
