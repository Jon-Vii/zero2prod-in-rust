use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    let app = spawn_app().await;

    let response = app
        .post_login(&serde_json::json!({
            "username": "random-username",
            "password": "random-password",
        }))
        .await;

    assert_is_redirect_to(&response, "/login");

    let html_page = app.get_login_html().await;
    assert!(html_page.contains("<p><i>Authentication failed</i></p>"));

    let html_page = app.get_login_html().await;
    assert!(!html_page.contains("Authentication failed"));
}
