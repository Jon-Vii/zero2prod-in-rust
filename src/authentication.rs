use anyhow::Context;
use argon2::password_hash::{PasswordHash, SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

pub async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<Uuid, AuthError> {
    let mut user_id = None;
    let mut expected_password_hash = SecretString::from(
        "$argon2id$v=19$m=15000,t=2,p=1$\
        gZiV/M1gPc22ElAH/Jh1Hw$\
        CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
            .to_string(),
    );

    if let Some((stored_user_id, stored_password_hash)) =
        get_stored_credentials(&credentials.username, pool).await?
    {
        user_id = Some(stored_user_id);
        expected_password_hash = stored_password_hash;
    }

    tokio::task::spawn_blocking(move || {
        verify_password_hash(expected_password_hash, credentials.password)
    })
    .await
    .context("failed to spawn blocking password verification task")??;

    user_id
        .ok_or_else(|| anyhow::anyhow!("unknown username"))
        .map_err(AuthError::InvalidCredentials)
}

async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<(Uuid, SecretString)>, anyhow::Error> {
    let row = sqlx::query(
        r#"
        SELECT user_id, password_hash
        FROM users
        WHERE username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("failed to fetch stored credentials")?
    .map(|row| {
        let user_id: Uuid = row.get("user_id");
        let password_hash: String = row.get("password_hash");
        (user_id, SecretString::from(password_hash))
    });

    Ok(row)
}

fn verify_password_hash(
    expected_password_hash: SecretString,
    password_candidate: SecretString,
) -> Result<(), AuthError> {
    let expected_password_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .context("failed to parse password hash")?;

    Argon2::default()
        .verify_password(
            password_candidate.expose_secret().as_bytes(),
            &expected_password_hash,
        )
        .context("invalid password")
        .map_err(AuthError::InvalidCredentials)
}

pub async fn change_password(
    user_id: Uuid,
    password: SecretString,
    pool: &PgPool,
) -> Result<(), anyhow::Error> {
    let password_hash = tokio::task::spawn_blocking(move || compute_password_hash(password))
        .await
        .context("failed to spawn blocking password hashing task")?
        .context("failed to hash password")?;

    sqlx::query(
        r#"
        UPDATE users
        SET password_hash = $1
        WHERE user_id = $2
        "#,
    )
    .bind(password_hash.expose_secret())
    .bind(user_id)
    .execute(pool)
    .await
    .context("failed to change user's password")?;

    Ok(())
}

fn compute_password_hash(password: SecretString) -> Result<SecretString, anyhow::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(15000, 2, 1, None).unwrap(),
    )
    .hash_password(password.expose_secret().as_bytes(), &salt)?
    .to_string();
    Ok(SecretString::from(password_hash))
}
