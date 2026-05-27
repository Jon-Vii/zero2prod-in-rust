use crate::{
    ApplicationState,
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
};
use axum::{
    extract::{Form, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct SubscribeForm {
    name: String,
    email: String,
}

#[derive(Deserialize)]
pub struct ConfirmationParameters {
    token: String,
}

#[derive(Debug, Error)]
pub enum SubscribeError {
    #[error("invalid subscriber data")]
    ValidationError,
    #[error("failed to store subscription details")]
    StoreTokenError(#[source] sqlx::Error),
    #[error("failed to send confirmation email")]
    SendEmailError(#[source] reqwest::Error),
}

impl IntoResponse for SubscribeError {
    fn into_response(self) -> Response {
        match self {
            Self::ValidationError => StatusCode::BAD_REQUEST.into_response(),
            Self::StoreTokenError(error) => {
                tracing::error!(%error, "Failed to store subscription details");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::SendEmailError(error) => {
                tracing::error!(%error, "Failed to send a confirmation email");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfirmError {
    #[error("invalid subscription token")]
    InvalidToken,
    #[error("failed to fetch subscription token")]
    TokenLookupError(#[source] sqlx::Error),
    #[error("failed to confirm subscriber")]
    ConfirmationError(#[source] sqlx::Error),
}

impl IntoResponse for ConfirmError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidToken => StatusCode::UNAUTHORIZED.into_response(),
            Self::TokenLookupError(error) => {
                tracing::error!(%error, "Failed to fetch subscription token");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::ConfirmationError(error) => {
                tracing::error!(%error, "Failed to confirm subscriber");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

pub async fn subscribe(
    State(state): State<ApplicationState>,
    Form(form): Form<SubscribeForm>,
) -> Result<StatusCode, SubscribeError> {
    let new_subscriber =
        NewSubscriber::try_from(form).map_err(|_| SubscribeError::ValidationError)?;

    tracing::info!("Saving a new subscriber");

    let subscriber_id = Uuid::new_v4();
    let subscription_token = generate_subscription_token();

    store_subscription(
        &state.db_pool,
        &new_subscriber,
        subscriber_id,
        &subscription_token,
    )
    .await
    .map_err(SubscribeError::StoreTokenError)?;

    let confirmation_link = confirmation_link(&state.application_base_url, &subscription_token);
    let (html_body, plain_body) = confirmation_email_body(&confirmation_link);

    state
        .email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
        .map_err(SubscribeError::SendEmailError)?;

    Ok(StatusCode::OK)
}

async fn store_subscription(
    pool: &PgPool,
    new_subscriber: &NewSubscriber,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;

    insert_subscriber(&mut transaction, new_subscriber, subscriber_id).await?;
    store_token(&mut transaction, subscriber_id, subscription_token).await?;

    transaction.commit().await
}

async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
    subscriber_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, now(), 'pending_confirmation')
        "#,
    )
    .bind(subscriber_id)
    .bind(new_subscriber.email.as_ref())
    .bind(new_subscriber.name.as_ref())
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)
        "#,
    )
    .bind(subscription_token)
    .bind(subscriber_id)
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

fn generate_subscription_token() -> String {
    Uuid::new_v4().simple().to_string()
}

fn confirmation_link(application_base_url: &str, subscription_token: &str) -> String {
    format!("{application_base_url}/subscriptions/confirm?token={subscription_token}")
}

fn confirmation_email_body(confirmation_link: &str) -> (String, String) {
    let plain_body = format!("Welcome to our newsletter.\nVisit {confirmation_link} to confirm.");
    let html_body = format!(
        "Welcome to our newsletter.<br />Click <a href=\"{confirmation_link}\">here</a> to confirm."
    );

    (html_body, plain_body)
}

pub async fn confirm(
    State(state): State<ApplicationState>,
    Query(parameters): Query<ConfirmationParameters>,
) -> Result<StatusCode, ConfirmError> {
    let subscriber_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT subscriber_id
        FROM subscription_tokens
        WHERE subscription_token = $1
        "#,
    )
    .bind(parameters.token)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(ConfirmError::TokenLookupError)?
    .ok_or(ConfirmError::InvalidToken)?;

    sqlx::query(
        r#"
        UPDATE subscriptions
        SET status = 'confirmed'
        WHERE id = $1
        "#,
    )
    .bind(subscriber_id)
    .execute(&state.db_pool)
    .await
    .map_err(ConfirmError::ConfirmationError)?;

    Ok(StatusCode::OK)
}

impl TryFrom<SubscribeForm> for NewSubscriber {
    type Error = String;

    fn try_from(form: SubscribeForm) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(form.name)?;
        let email = SubscriberEmail::parse(form.email)?;

        Ok(Self { email, name })
    }
}
