use crate::{domain::SubscriberEmail, email_client::EmailClient};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::time::Duration;
use uuid::Uuid;

pub async fn run_worker_until_stopped(pool: PgPool, email_client: EmailClient) -> ! {
    loop {
        match try_execute_task(&pool, &email_client).await {
            Ok(ExecutionOutcome::EmptyQueue) => {
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            Ok(ExecutionOutcome::TaskCompleted) => {}
            Err(error) => {
                tracing::error!(%error, "Failed to execute issue delivery task");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

pub enum ExecutionOutcome {
    TaskCompleted,
    EmptyQueue,
}

pub async fn try_execute_task(
    pool: &PgPool,
    email_client: &EmailClient,
) -> Result<ExecutionOutcome, anyhow::Error> {
    let Some((transaction, issue_id, email)) = dequeue_task(pool).await? else {
        return Ok(ExecutionOutcome::EmptyQueue);
    };

    if let Ok(email) = SubscriberEmail::parse(email.clone()) {
        let issue = get_issue(pool, issue_id).await?;
        if let Err(error) = email_client
            .send_email(
                email,
                &issue.title,
                &issue.html_content,
                &issue.text_content,
            )
            .await
        {
            tracing::error!(%error, "Failed to deliver newsletter issue");
        }
    } else {
        tracing::error!(subscriber_email = %email, "Skipping invalid subscriber email");
    }

    delete_task(transaction, issue_id, &email).await?;
    Ok(ExecutionOutcome::TaskCompleted)
}

type PgTransaction = Transaction<'static, Postgres>;

async fn dequeue_task(
    pool: &PgPool,
) -> Result<Option<(PgTransaction, Uuid, String)>, anyhow::Error> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT newsletter_issue_id, subscriber_email
        FROM issue_delivery_queue
        FOR UPDATE
        SKIP LOCKED
        LIMIT 1
        "#,
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some((
        transaction,
        row.get("newsletter_issue_id"),
        row.get("subscriber_email"),
    )))
}

async fn delete_task(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    email: &str,
) -> Result<(), anyhow::Error> {
    sqlx::query(
        r#"
        DELETE FROM issue_delivery_queue
        WHERE newsletter_issue_id = $1 AND subscriber_email = $2
        "#,
    )
    .bind(issue_id)
    .bind(email)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

struct NewsletterIssue {
    title: String,
    text_content: String,
    html_content: String,
}

async fn get_issue(pool: &PgPool, issue_id: Uuid) -> Result<NewsletterIssue, anyhow::Error> {
    let row = sqlx::query(
        r#"
        SELECT title, text_content, html_content
        FROM newsletter_issues
        WHERE newsletter_issue_id = $1
        "#,
    )
    .bind(issue_id)
    .fetch_one(pool)
    .await?;

    Ok(NewsletterIssue {
        title: row.get("title"),
        text_content: row.get("text_content"),
        html_content: row.get("html_content"),
    })
}
