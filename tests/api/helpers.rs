use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version};
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
    issue_delivery_worker::{ExecutionOutcome, try_execute_task},
};

pub struct TestApp {
    pub address: String,
    db_pool: PgPool,
    email_server: MockServer,
    client: reqwest::Client,
    email_client: EmailClient,
    pub test_user: TestUser,
}

pub struct SavedSubscription {
    pub email: String,
    pub name: String,
    pub status: String,
}

impl TestApp {
    pub async fn dispatch_all_pending_emails(&self) {
        loop {
            let outcome = try_execute_task(&self.db_pool, &self.email_client)
                .await
                .expect("failed to execute pending email task");
            if matches!(outcome, ExecutionOutcome::EmptyQueue) {
                break;
            }
        }
    }

    pub async fn get_health_check(&self) -> reqwest::Response {
        self.client
            .get(format!("{}/health_check", self.address))
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        self.client
            .post(format!("{}/subscriptions", self.address))
            .header("Content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn post_login<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize + ?Sized,
    {
        self.client
            .post(format!("{}/login", self.address))
            .form(body)
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn get_login_html(&self) -> String {
        self.client
            .get(format!("{}/login", self.address))
            .send()
            .await
            .expect("failed to execute request")
            .text()
            .await
            .expect("failed to read response body")
    }

    pub async fn get_admin_dashboard(&self) -> reqwest::Response {
        self.client
            .get(format!("{}/admin/dashboard", self.address))
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn get_admin_dashboard_html(&self) -> String {
        self.get_admin_dashboard()
            .await
            .text()
            .await
            .expect("failed to read response body")
    }

    pub async fn post_logout(&self) -> reqwest::Response {
        self.client
            .post(format!("{}/admin/logout", self.address))
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn get_publish_newsletter(&self) -> reqwest::Response {
        self.client
            .get(format!("{}/admin/newsletters", self.address))
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn post_publish_newsletter<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize + ?Sized,
    {
        self.client
            .post(format!("{}/admin/newsletters", self.address))
            .form(body)
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn get_change_password(&self) -> reqwest::Response {
        self.client
            .get(format!("{}/admin/password", self.address))
            .send()
            .await
            .expect("failed to execute request")
    }

    pub async fn get_change_password_html(&self) -> String {
        self.get_change_password()
            .await
            .text()
            .await
            .expect("failed to read response body")
    }

    pub async fn post_change_password<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize + ?Sized,
    {
        self.client
            .post(format!("{}/admin/password", self.address))
            .form(body)
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
        self.client
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
        email_client: email_client.clone(),
        application_base_url: format!("http://{address}"),
    };
    let test_user = TestUser::generate();
    test_user.store(&connection_pool).await;
    let hmac_secret = "0123456789012345678901234567890123456789012345678901234567890123";

    tokio::spawn(zero2prod::run(
        listener,
        state,
        tower_sessions::MemoryStore::default(),
        hmac_secret.as_bytes(),
    ));

    TestApp {
        address: format!("http://{address}"),
        db_pool: connection_pool,
        email_server,
        email_client,
        client: reqwest::Client::builder()
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build test HTTP client"),
        test_user,
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

pub struct TestUser {
    user_id: Uuid,
    pub username: String,
    pub password: String,
}

impl TestUser {
    fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
        }
    }

    pub async fn login(&self, app: &TestApp) {
        app.post_login(&serde_json::json!({
            "username": &self.username,
            "password": &self.password,
        }))
        .await;
    }

    async fn store(&self, pool: &PgPool) {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(15000, 2, 1, None).unwrap(),
        )
        .hash_password(self.password.as_bytes(), &salt)
        .expect("failed to hash test user's password")
        .to_string();

        sqlx::query(
            r#"
            INSERT INTO users (user_id, username, password_hash)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(self.user_id)
        .bind(&self.username)
        .bind(password_hash)
        .execute(pool)
        .await
        .expect("failed to store test user");
    }
}

pub fn assert_is_redirect_to(response: &reqwest::Response, location: &str) {
    assert_eq!(response.status().as_u16(), 303);
    assert_eq!(response.headers().get("Location").unwrap(), location);
}
