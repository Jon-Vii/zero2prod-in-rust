use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use axum::{
    extract::{Form, State},
    http::StatusCode,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct SubscribeForm {
    name: String,
    email: String,
}

pub async fn subscribe(State(pool): State<PgPool>, Form(form): Form<SubscribeForm>) -> StatusCode {
    let new_subscriber = match NewSubscriber::try_from(form) {
        Ok(subscriber) => subscriber,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    tracing::info!("Saving a new subscriber");

    let result = sqlx::query(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, now())
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(new_subscriber.email.as_ref())
    .bind(new_subscriber.name.as_ref())
    .execute(&pool)
    .await;

    match result {
        Ok(_) => StatusCode::OK,
        Err(error) => {
            tracing::error!(%error, "Failed to save new subscriber");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

impl TryFrom<SubscribeForm> for NewSubscriber {
    type Error = String;

    fn try_from(form: SubscribeForm) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(form.name)?;
        let email = SubscriberEmail::parse(form.email)?;

        Ok(Self { email, name })
    }
}
