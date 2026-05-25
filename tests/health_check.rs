use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};
use uuid::Uuid;
use zero2prod::configuration::{DatabaseSettings, get_configuration};

struct TestApp {
    address: String,
    db_pool: PgPool,
}

async fn spawn_app() -> TestApp {
    zero2prod::telemetry::init_test_subscriber();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind random port");
    let address = listener.local_addr().expect("failed to get local address");

    let mut configuration = get_configuration().expect("failed to read configuration");
    configuration.database.database_name = Uuid::new_v4().simple().to_string();
    let connection_pool = configure_database(&configuration.database).await;

    tokio::spawn(zero2prod::run(listener, connection_pool.clone()));

    TestApp {
        address: format!("http://{address}"),
        db_pool: connection_pool,
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

#[tokio::test]
async fn health_check_works() {
    let app = spawn_app().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/health_check", app.address))
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
    assert!(response.headers().contains_key("x-request-id"));
}

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    let app = spawn_app().await;

    let client = reqwest::Client::new();

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    let response = client
        .post(format!("{}/subscriptions", app.address))
        .header("Content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let saved = sqlx::query!("SELECT email, name FROM subscriptions")
        .fetch_one(&app.db_pool)
        .await
        .expect("failed to fetch saved subscription");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    let app = spawn_app().await;

    let client = reqwest::Client::new();

    let testcases = vec![
        ("name=le%20guin", "missing email"),
        ("email=ursula_le_guin%40gmail.com", "missing name"),
        ("", "missing both email and name"),
    ];

    for (invalid_body, _error_message) in testcases {
        let response = client
            .post(format!("{}/subscriptions", app.address))
            .header("Content-type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("failed to execute request");

        assert!(response.status().is_client_error());
    }
}
