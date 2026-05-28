use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    let app = spawn_app().await;
    app.subscribe_confirmed("confirmed@example.com").await;
    let setup_email_count = app.email_requests().await.len();

    app.test_user.login(&app).await;
    let response = app
        .post_publish_newsletter(&serde_json::json!({
            "title": "Newsletter title",
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as HTML</p>",
            "idempotency_key": uuid::Uuid::new_v4().to_string()
        }))
        .await;

    assert_is_redirect_to(&response, "/admin/newsletters");
    app.dispatch_all_pending_emails().await;

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), setup_email_count + 1);

    let email_request: serde_json::Value =
        serde_json::from_slice(&email_requests.last().unwrap().body)
            .expect("failed to parse email request body");

    assert_eq!(email_request["To"], "confirmed@example.com");
    assert_eq!(email_request["Subject"], "Newsletter title");
    assert_eq!(email_request["TextBody"], "Newsletter body as plain text");
    assert_eq!(email_request["HtmlBody"], "<p>Newsletter body as HTML</p>");
}

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    let app = spawn_app().await;
    app.subscribe_confirmed("confirmed@example.com").await;
    app.subscribe_unconfirmed("unconfirmed@example.com").await;
    let setup_email_count = app.email_requests().await.len();

    app.test_user.login(&app).await;
    let response = app
        .post_publish_newsletter(&serde_json::json!({
            "title": "Newsletter title",
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as HTML</p>",
            "idempotency_key": uuid::Uuid::new_v4().to_string()
        }))
        .await;

    assert_is_redirect_to(&response, "/admin/newsletters");
    app.dispatch_all_pending_emails().await;

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), setup_email_count + 1);

    let email_request: serde_json::Value =
        serde_json::from_slice(&email_requests.last().unwrap().body)
            .expect("failed to parse email request body");

    assert_eq!(email_request["To"], "confirmed@example.com");
}

#[tokio::test]
async fn you_must_be_logged_in_to_see_the_newsletter_form() {
    let app = spawn_app().await;

    let response = app.get_publish_newsletter().await;

    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_be_logged_in_to_publish_a_newsletter() {
    let app = spawn_app().await;

    let response = app
        .post_publish_newsletter(&serde_json::json!({
            "title": "Newsletter title",
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as HTML</p>",
            "idempotency_key": uuid::Uuid::new_v4().to_string()
        }))
        .await;

    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn newsletter_creation_is_idempotent() {
    let app = spawn_app().await;
    app.subscribe_confirmed("confirmed@example.com").await;
    let setup_email_count = app.email_requests().await.len();
    app.test_user.login(&app).await;

    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string()
    });

    let response = app.post_publish_newsletter(&newsletter_request_body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");

    let response = app.post_publish_newsletter(&newsletter_request_body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");

    app.dispatch_all_pending_emails().await;

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), setup_email_count + 1);
}

#[tokio::test]
async fn concurrent_form_submission_is_handled_gracefully() {
    let app = spawn_app().await;
    app.subscribe_confirmed("confirmed@example.com").await;
    let setup_email_count = app.email_requests().await.len();
    app.test_user.login(&app).await;

    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "text_content": "Newsletter body as plain text",
        "html_content": "<p>Newsletter body as HTML</p>",
        "idempotency_key": uuid::Uuid::new_v4().to_string()
    });

    let response1 = app.post_publish_newsletter(&newsletter_request_body);
    let response2 = app.post_publish_newsletter(&newsletter_request_body);
    let (response1, response2) = tokio::join!(response1, response2);

    assert_eq!(response1.status(), response2.status());
    assert_eq!(
        response1.headers().get("Location"),
        response2.headers().get("Location")
    );

    app.dispatch_all_pending_emails().await;

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), setup_email_count + 1);
}
