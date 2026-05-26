use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::env;

#[derive(Deserialize)]
pub struct Settings {
    pub application_host: String,
    pub application_port: u16,
    pub application_base_url: String,
    pub database: DatabaseSettings,
    pub email_client: EmailClientSettings,
}

#[derive(Deserialize)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: SecretString,
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
}

#[derive(Deserialize)]
pub struct EmailClientSettings {
    pub base_url: String,
    pub sender_email: String,
    pub authorization_token: SecretString,
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port,
            self.database_name,
            self.ssl_mode()
        )
    }

    pub fn connection_string_without_db(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}?sslmode={}",
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port,
            self.ssl_mode()
        )
    }

    fn ssl_mode(&self) -> &'static str {
        if self.require_ssl {
            "require"
        } else {
            "prefer"
        }
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    let environment = env::var("APP_ENVIRONMENT").unwrap_or_else(|_| "local".into());
    let environment_filename = format!("configuration/{environment}");

    let settings = config::Config::builder()
        .add_source(config::File::with_name("configuration/base"))
        .add_source(config::File::with_name(&environment_filename))
        .add_source(
            config::Environment::with_prefix("APP")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()?;

    settings.try_deserialize::<Settings>()
}
