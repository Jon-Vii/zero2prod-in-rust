#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let configuration = zero2prod::configuration::get_configuration()?;
    let address = format!("127.0.0.1:{}", configuration.application_port);
    let connection_pool =
        sqlx::PgPool::connect(&configuration.database.connection_string()).await?;

    zero2prod::run_on(&address, connection_pool).await?;

    Ok(())
}
