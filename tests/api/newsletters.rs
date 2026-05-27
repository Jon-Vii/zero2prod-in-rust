use crate::helpers::spawn_app;

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    let app = spawn_app().await;
    app.subscribe_confirmed("confirmed@example.com").await;
    let setup_email_count = app.email_requests().await.len();

    let response = app
        .post_newsletters(serde_json::json!({
            "title": "Newsletter title",
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as HTML</p>"
        }))
        .await;

    assert!(response.status().is_success());

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

    let response = app
        .post_newsletters(serde_json::json!({
            "title": "Newsletter title",
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as HTML</p>"
        }))
        .await;

    assert!(response.status().is_success());

    let email_requests = app.email_requests().await;
    assert_eq!(email_requests.len(), setup_email_count + 1);

    let email_request: serde_json::Value =
        serde_json::from_slice(&email_requests.last().unwrap().body)
            .expect("failed to parse email request body");

    assert_eq!(email_request["To"], "confirmed@example.com");
}

#[tokio::test]
async fn newsletters_return_400_for_invalid_json() {
    let app = spawn_app().await;

    let test_cases = vec![
        serde_json::json!({
            "text_content": "Newsletter body as plain text",
            "html_content": "<p>Newsletter body as HTML</p>"
        }),
        serde_json::json!({
            "title": "Newsletter title",
            "html_content": "<p>Newsletter body as HTML</p>"
        }),
        serde_json::json!({
            "title": "Newsletter title",
            "text_content": "Newsletter body as plain text"
        }),
    ];

    for invalid_body in test_cases {
        let response = app.post_newsletters(invalid_body).await;

        assert_eq!(response.status().as_u16(), 400);
    }
}
