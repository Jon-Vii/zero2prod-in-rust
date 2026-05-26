#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    zero2prod::telemetry::init_subscriber();

    let configuration = zero2prod::configuration::get_configuration()?;
    let address = format!(
        "{}:{}",
        configuration.application_host, configuration.application_port
    );
    let connection_pool =
        sqlx::PgPool::connect(&configuration.database.connection_string()).await?;
    let sender_email =
        zero2prod::domain::SubscriberEmail::parse(configuration.email_client.sender_email)?;
    let email_client = zero2prod::email_client::EmailClient::new(
        configuration.email_client.base_url,
        sender_email,
        configuration.email_client.authorization_token,
    )?;
    let state = zero2prod::ApplicationState {
        db_pool: connection_pool,
        email_client,
        application_base_url: configuration.application_base_url,
    };

    zero2prod::run_on(&address, state).await?;

    Ok(())
}
