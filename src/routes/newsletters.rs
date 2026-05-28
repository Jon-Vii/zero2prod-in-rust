use crate::{
    ApplicationState,
    idempotency::{IdempotencyKey, NextAction, save_response, try_processing},
};
use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::{Postgres, Transaction};
use thiserror::Error;
use tower_sessions::Session;
use uuid::Uuid;

use super::admin::{require_login, set_flash};

#[derive(Deserialize)]
pub struct Newsletter {
    title: String,
    text_content: String,
    html_content: String,
    idempotency_key: String,
}

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("invalid idempotency key")]
    InvalidIdempotencyKey(#[source] anyhow::Error),
    #[error("failed to publish newsletter issue")]
    PublishIssueError(#[source] anyhow::Error),
}

impl IntoResponse for PublishError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidIdempotencyKey(error) => {
                tracing::warn!(%error, "Invalid idempotency key");
                StatusCode::BAD_REQUEST.into_response()
            }
            Self::PublishIssueError(error) => {
                tracing::error!(%error, "Failed to publish newsletter issue");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

pub async fn admin_publish_newsletter(
    State(state): State<ApplicationState>,
    session: Session,
    Form(newsletter): Form<Newsletter>,
) -> Result<Response, Response> {
    let user_id = require_login(&session).await?;
    let idempotency_key = IdempotencyKey::try_from(newsletter.idempotency_key.clone())
        .map_err(|error| PublishError::InvalidIdempotencyKey(error).into_response())?;

    match try_processing(&state.db_pool, &idempotency_key, user_id)
        .await
        .map_err(|error| PublishError::PublishIssueError(error).into_response())?
    {
        NextAction::StartProcessing(mut transaction) => {
            enqueue_newsletter_issue(&mut transaction, &newsletter)
                .await
                .map_err(|error| PublishError::PublishIssueError(error).into_response())?;
            let response =
                save_response(transaction, &idempotency_key, user_id, "/admin/newsletters")
                    .await
                    .map_err(|error| PublishError::PublishIssueError(error).into_response())?;
            set_flash(
                &session,
                "The newsletter issue has been accepted - emails will go out shortly.",
            )
            .await;
            Ok(response)
        }
        NextAction::ReturnSavedResponse(response) => {
            set_flash(
                &session,
                "The newsletter issue has been accepted - emails will go out shortly.",
            )
            .await;
            Ok(response)
        }
    }
}

async fn enqueue_newsletter_issue(
    transaction: &mut Transaction<'static, Postgres>,
    newsletter: &Newsletter,
) -> Result<(), anyhow::Error> {
    let issue_id = insert_newsletter_issue(transaction, newsletter).await?;
    enqueue_delivery_tasks(transaction, issue_id).await?;
    Ok(())
}

async fn insert_newsletter_issue(
    transaction: &mut Transaction<'_, Postgres>,
    newsletter: &Newsletter,
) -> Result<Uuid, sqlx::Error> {
    let issue_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO newsletter_issues (
            newsletter_issue_id,
            title,
            text_content,
            html_content,
            published_at
        )
        VALUES ($1, $2, $3, $4, now())
        "#,
    )
    .bind(issue_id)
    .bind(&newsletter.title)
    .bind(&newsletter.text_content)
    .bind(&newsletter.html_content)
    .execute(&mut **transaction)
    .await?;

    Ok(issue_id)
}

async fn enqueue_delivery_tasks(
    transaction: &mut Transaction<'_, Postgres>,
    issue_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO issue_delivery_queue (newsletter_issue_id, subscriber_email)
        SELECT $1, email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .bind(issue_id)
    .execute(&mut **transaction)
    .await?;

    Ok(())
}
