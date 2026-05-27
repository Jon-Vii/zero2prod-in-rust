use secrecy::SecretString;
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool, Row};
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};
use zero2prod::{
    ApplicationState,
    configuration::{DatabaseSettings, get_configuration},
    domain::SubscriberEmail,
    email_client::EmailClient,
};

pub struct TestApp {
    pub address: String,
    db_pool: PgPool,
    email_server: MockServer,
}

pub struct SavedSubscription {
    pub email: String,
    pub name: String,
    pub status: String,
}

impl TestApp {
    pub async fn get_health_check(&self) -> reqwest::Response {
        reqwest::Client::new()
            .get(format!("{}/health_check", self.address))
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/subscriptions", self.address))
            .header("Content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn post_newsletters(&self, body: serde_json::Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/newsletters", self.address))
            .json(&body)
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn saved_subscription(&self) -> SavedSubscription {
        let saved = sqlx::query("SELECT email, name, status FROM subscriptions")
            .fetch_one(&self.db_pool)
            .await
            .expect("failed to fetch saved subscription");

        SavedSubscription {
            email: saved.get("email"),
            name: saved.get("name"),
            status: saved.get("status"),
        }
    }

    pub async fn saved_subscription_token(&self, subscriber_email: &str) -> String {
        let saved = sqlx::query(
            r#"
            SELECT subscription_token
            FROM subscription_tokens
            INNER JOIN subscriptions ON subscription_tokens.subscriber_id = subscriptions.id
            WHERE subscriptions.email = $1
            "#,
        )
        .bind(subscriber_email)
        .fetch_one(&self.db_pool)
        .await
        .expect("failed to fetch saved subscription token");

        saved.get("subscription_token")
    }

    pub async fn email_requests(&self) -> Vec<Request> {
        self.email_server
            .received_requests()
            .await
            .expect("failed to fetch email requests")
    }

    pub async fn get_subscription_confirmation(
        &self,
        confirmation_link: &str,
    ) -> reqwest::Response {
        reqwest::Client::new()
            .get(confirmation_link)
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn subscribe_confirmed(&self, email: &str) {
        self.subscribe_unconfirmed(email).await;

        let email_requests = self.email_requests().await;
        let confirmation_link = get_confirmation_link(
            email_requests
                .last()
                .expect("no confirmation email was sent"),
        );

        let response = self.get_subscription_confirmation(&confirmation_link).await;
        assert!(response.status().is_success());
    }

    pub async fn subscribe_unconfirmed(&self, email: &str) {
        let body = format!("name=Test%20User&email={}", email.replace('@', "%40"));
        let response = self.post_subscriptions(body).await;
        assert!(response.status().is_success());
    }
}

pub async fn spawn_app() -> TestApp {
    spawn_app_with_email_status(200).await
}

pub async fn spawn_app_with_email_status(email_status_code: u16) -> TestApp {
    zero2prod::telemetry::init_test_subscriber();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind random port");
    let address = listener.local_addr().expect("failed to get local address");

    let mut configuration = get_configuration().expect("failed to read configuration");
    configuration.database.database_name = Uuid::new_v4().simple().to_string();
    let connection_pool = configure_database(&configuration.database).await;
    let email_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/email"))
        .respond_with(ResponseTemplate::new(email_status_code))
        .mount(&email_server)
        .await;
    let sender_email = SubscriberEmail::parse(configuration.email_client.sender_email)
        .expect("failed to parse sender email");
    let email_client = EmailClient::new(
        email_server.uri(),
        sender_email,
        SecretString::from("my-secret-token"),
    )
    .expect("failed to build email client");
    let state = ApplicationState {
        db_pool: connection_pool.clone(),
        email_client,
        application_base_url: format!("http://{address}"),
    };

    tokio::spawn(zero2prod::run(listener, state));

    TestApp {
        address: format!("http://{address}"),
        db_pool: connection_pool,
        email_server,
    }
}

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect(&config.connection_string_without_db())
        .await
        .expect("failed to connect to Postgres");

    let create_database_query = format!(r#"CREATE DATABASE "{}";"#, config.database_name);
    sqlx::query(AssertSqlSafe(create_database_query))
        .execute(&mut connection)
        .await
        .expect("failed to create database");

    let connection_pool = PgPool::connect(&config.connection_string())
        .await
        .expect("failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("failed to migrate the database");

    connection_pool
}

pub fn get_confirmation_link(request: &Request) -> String {
    let body: serde_json::Value =
        serde_json::from_slice(&request.body).expect("failed to parse email request body");
    let text_body = body["TextBody"]
        .as_str()
        .expect("email request body did not contain TextBody");

    text_body
        .split_whitespace()
        .find(|value| value.contains("/subscriptions/confirm?token="))
        .expect("confirmation link was not found in the email body")
        .to_string()
}
