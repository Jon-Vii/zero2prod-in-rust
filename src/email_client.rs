use crate::domain::SubscriberEmail;
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;

#[derive(Clone)]
pub struct EmailClient {
    http_client: reqwest::Client,
    base_url: Url,
    sender: SubscriberEmail,
    authorization_token: SecretString,
}

impl EmailClient {
    pub fn new(
        base_url: String,
        sender: SubscriberEmail,
        authorization_token: SecretString,
    ) -> Result<Self, String> {
        Ok(Self {
            http_client: reqwest::Client::new(),
            base_url: Url::parse(&base_url).map_err(|error| error.to_string())?,
            sender,
            authorization_token,
        })
    }

    pub async fn send_email(
        &self,
        recipient: SubscriberEmail,
        subject: &str,
        html_content: &str,
        text_content: &str,
    ) -> Result<(), reqwest::Error> {
        let url = self.base_url.join("email").expect("Invalid base URL");
        let request_body = SendEmailRequest {
            from: self.sender.as_ref(),
            to: recipient.as_ref(),
            subject,
            html_body: html_content,
            text_body: text_content,
        };

        self.http_client
            .post(url)
            .header(
                "X-Postmark-Server-Token",
                self.authorization_token.expose_secret(),
            )
            .json(&request_body)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SendEmailRequest<'a> {
    from: &'a str,
    to: &'a str,
    subject: &'a str,
    html_body: &'a str,
    text_body: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    #[tokio::test]
    async fn send_email_sends_expected_request() {
        let mock_server = MockServer::start().await;
        let sender = SubscriberEmail::parse("sender@example.com".to_string()).unwrap();
        let recipient = SubscriberEmail::parse("recipient@example.com".to_string()).unwrap();
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender,
            SecretString::from("my-secret-token"),
        )
        .unwrap();

        Mock::given(method("POST"))
            .and(path("/email"))
            .and(header("X-Postmark-Server-Token", "my-secret-token"))
            .and(body_json(serde_json::json!({
                "From": "sender@example.com",
                "To": "recipient@example.com",
                "Subject": "Welcome!",
                "HtmlBody": "<p>Welcome.</p>",
                "TextBody": "Welcome."
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        email_client
            .send_email(recipient, "Welcome!", "<p>Welcome.</p>", "Welcome.")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn send_email_returns_error_on_server_failure() {
        let mock_server = MockServer::start().await;
        let sender = SubscriberEmail::parse("sender@example.com".to_string()).unwrap();
        let recipient = SubscriberEmail::parse("recipient@example.com".to_string()).unwrap();
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender,
            SecretString::from("my-secret-token"),
        )
        .unwrap();

        Mock::given(method("POST"))
            .and(path("/email"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let error = email_client
            .send_email(recipient, "Welcome!", "<p>Welcome.</p>", "Welcome.")
            .await
            .expect_err("Expected send_email to fail");

        assert!(error.is_status());
    }
}
