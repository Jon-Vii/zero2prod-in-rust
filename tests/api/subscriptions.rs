use crate::helpers::{get_confirmation_link, spawn_app, spawn_app_with_email_status};
use wiremock::Request;

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    let app = spawn_app().await;

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    let response = app.post_subscriptions(body.into()).await;

    assert!(response.status().is_success());

    let saved = app.saved_subscription().await;

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "pending_confirmation");

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), 1);
    assert_email_request_matches(&email_requests[0]);
}

fn assert_email_request_matches(request: &Request) {
    let body: serde_json::Value =
        serde_json::from_slice(&request.body).expect("failed to parse email request body");

    assert_eq!(request.method.as_str(), "POST");
    assert_eq!(request.url.path(), "/email");
    assert_eq!(
        request.headers["X-Postmark-Server-Token"],
        "my-secret-token"
    );
    assert_eq!(body["From"], "test@gmail.com");
    assert_eq!(body["To"], "ursula_le_guin@gmail.com");
    assert_eq!(body["Subject"], "Welcome!");
    assert!(
        body["HtmlBody"]
            .as_str()
            .unwrap()
            .contains("/subscriptions/confirm?token=")
    );
    assert!(
        body["TextBody"]
            .as_str()
            .unwrap()
            .contains("/subscriptions/confirm?token=")
    );
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    let app = spawn_app().await;

    let testcases = vec![
        ("name=le%20guin", "missing email"),
        ("email=ursula_le_guin%40gmail.com", "missing name"),
        ("", "missing both email and name"),
    ];

    for (invalid_body, _error_message) in testcases {
        let response = app.post_subscriptions(invalid_body.into()).await;

        assert!(response.status().is_client_error());
    }

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), 0);
}

#[tokio::test]
async fn subscribe_returns_a_400_when_fields_are_present_but_invalid() {
    let app = spawn_app().await;

    let testcases = vec![
        ("name=&email=ursula_le_guin%40gmail.com", "empty name"),
        ("name=le%20guin&email=", "empty email"),
        ("name=le%20guin&email=not-an-email", "invalid email"),
    ];

    for (invalid_body, _error_message) in testcases {
        let response = app.post_subscriptions(invalid_body.into()).await;

        assert!(response.status().is_client_error());
    }

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), 0);
}

#[tokio::test]
async fn subscribe_returns_a_500_if_confirmation_email_fails() {
    let app = spawn_app_with_email_status(500).await;

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    let response = app.post_subscriptions(body.into()).await;

    assert!(response.status().is_server_error());

    let saved = app.saved_subscription().await;

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "pending_confirmation");

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), 1);
}

#[tokio::test]
async fn subscription_confirm_returns_a_200_for_a_valid_token() {
    let app = spawn_app().await;

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    app.post_subscriptions(body.into()).await;

    let email_requests = app.email_requests().await;
    let confirmation_link = get_confirmation_link(&email_requests[0]);

    let response = app.get_subscription_confirmation(&confirmation_link).await;

    assert!(response.status().is_success());

    let saved = app.saved_subscription().await;

    assert_eq!(saved.status, "confirmed");
}

#[tokio::test]
async fn subscription_confirm_returns_a_401_for_an_invalid_token() {
    let app = spawn_app().await;

    let response = app
        .get_subscription_confirmation(&format!(
            "{}/subscriptions/confirm?token=invalid-token",
            app.address
        ))
        .await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn subscription_confirm_returns_a_400_if_token_is_missing() {
    let app = spawn_app().await;

    let response = app
        .get_subscription_confirmation(&format!("{}/subscriptions/confirm", app.address))
        .await;

    assert_eq!(response.status().as_u16(), 400);
}
