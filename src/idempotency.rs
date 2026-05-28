use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct IdempotencyKey(String);

impl TryFrom<String> for IdempotencyKey {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            anyhow::bail!("The idempotency key cannot be empty");
        }
        if value.len() >= 50 {
            anyhow::bail!("The idempotency key must be shorter than 50 characters");
        }
        Ok(Self(value))
    }
}

impl AsRef<str> for IdempotencyKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub enum NextAction {
    StartProcessing(Transaction<'static, Postgres>),
    ReturnSavedResponse(Response),
}

pub async fn try_processing(
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    user_id: Uuid,
) -> Result<NextAction, anyhow::Error> {
    let mut transaction = pool.begin().await?;
    let rows_inserted = sqlx::query(
        r#"
        INSERT INTO idempotency (user_id, idempotency_key, created_at)
        VALUES ($1, $2, now())
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(idempotency_key.as_ref())
    .execute(&mut *transaction)
    .await?
    .rows_affected();

    if rows_inserted > 0 {
        return Ok(NextAction::StartProcessing(transaction));
    }

    for _ in 0..100 {
        if let Some(response) = get_saved_response(pool, idempotency_key, user_id).await? {
            return Ok(NextAction::ReturnSavedResponse(response));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    anyhow::bail!("Timed out waiting for the idempotent request to complete")
}

pub async fn get_saved_response(
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    user_id: Uuid,
) -> Result<Option<Response>, anyhow::Error> {
    let row = sqlx::query(
        r#"
        SELECT response_status_code, response_location
        FROM idempotency
        WHERE user_id = $1 AND idempotency_key = $2
        "#,
    )
    .bind(user_id)
    .bind(idempotency_key.as_ref())
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let status_code: Option<i16> = row.get("response_status_code");
    let location: Option<String> = row.get("response_location");

    match (status_code, location) {
        (Some(303), Some(location)) => {
            let mut response = StatusCode::SEE_OTHER.into_response();
            response
                .headers_mut()
                .insert(header::LOCATION, HeaderValue::from_str(&location)?);
            Ok(Some(response))
        }
        (Some(status_code), _) => Ok(Some(
            StatusCode::from_u16(status_code as u16)?.into_response(),
        )),
        _ => Ok(None),
    }
}

pub async fn save_response(
    mut transaction: Transaction<'static, Postgres>,
    idempotency_key: &IdempotencyKey,
    user_id: Uuid,
    location: &str,
) -> Result<Response, anyhow::Error> {
    sqlx::query(
        r#"
        UPDATE idempotency
        SET response_status_code = $3, response_location = $4
        WHERE user_id = $1 AND idempotency_key = $2
        "#,
    )
    .bind(user_id)
    .bind(idempotency_key.as_ref())
    .bind(StatusCode::SEE_OTHER.as_u16() as i16)
    .bind(location)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    let mut response = StatusCode::SEE_OTHER.into_response();
    response
        .headers_mut()
        .insert(header::LOCATION, location.parse()?);
    Ok(response)
}
