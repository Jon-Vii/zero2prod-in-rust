use crate::{
    ApplicationState,
    authentication::{AuthError, Credentials, change_password, validate_credentials},
    session_state::TypedSession,
};
use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tower_sessions::Session;

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: SecretString,
}

pub async fn login_form(session: Session) -> Html<String> {
    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Login</title>
</head>
<body>
    {}
    <form action="/login" method="post">
        <label>Username
            <input type="text" placeholder="Enter Username" name="username">
        </label>
        <label>Password
            <input type="password" placeholder="Enter Password" name="password">
        </label>
        <button type="submit">Login</button>
    </form>
</body>
</html>"#,
        flash_html(&session).await
    ))
}

pub async fn login(
    State(state): State<ApplicationState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Result<Response, Response> {
    let credentials = Credentials {
        username: form.username,
        password: form.password,
    };

    let user_id = match validate_credentials(credentials, &state.db_pool).await {
        Ok(user_id) => user_id,
        Err(AuthError::InvalidCredentials(error)) => {
            tracing::warn!(%error, "Authentication failed");
            set_flash(&session, "Authentication failed").await;
            return Ok(Redirect::to("/login").into_response());
        }
        Err(AuthError::UnexpectedError(error)) => {
            tracing::error!(%error, "Login failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };

    let session = TypedSession::new(session);
    session.renew().await.map_err(|error| {
        tracing::error!(%error, "Failed to renew session");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;
    session.insert_user_id(user_id).await.map_err(|error| {
        tracing::error!(%error, "Failed to write session");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;

    Ok(Redirect::to("/admin/dashboard").into_response())
}

pub async fn admin_dashboard(
    State(state): State<ApplicationState>,
    session: Session,
) -> Result<Html<String>, Response> {
    let user_id = require_login(&session).await?;
    let username = sqlx::query_scalar::<_, String>(
        r#"
        SELECT username
        FROM users
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, "Failed to fetch user details");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;

    Ok(Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><title>Admin dashboard</title></head>
<body>
    <p>Welcome {username}</p>
    <p><a href="/admin/newsletters">Publish newsletter</a></p>
    <p><a href="/admin/password">Change password</a></p>
    <form action="/admin/logout" method="post">
        <button type="submit">Logout</button>
    </form>
</body>
</html>"#
    )))
}

pub async fn logout(session: Session) -> Result<Response, Response> {
    TypedSession::new(session.clone())
        .log_out()
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to clear session");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;

    set_flash(&session, "You have successfully logged out.").await;
    Ok(Redirect::to("/login").into_response())
}

pub async fn publish_newsletter_form(session: Session) -> Result<Html<String>, Response> {
    require_login(&session).await?;
    let idempotency_key = uuid::Uuid::new_v4();
    Ok(Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><title>Publish newsletter</title></head>
<body>
    {}
    <form action="/admin/newsletters" method="post">
        <label>Title <input type="text" name="title"></label>
        <label>Plain text <textarea name="text_content"></textarea></label>
        <label>HTML <textarea name="html_content"></textarea></label>
        <input hidden type="text" name="idempotency_key" value="{idempotency_key}">
        <button type="submit">Publish</button>
    </form>
</body>
</html>"#,
        flash_html(&session).await
    )))
}

#[derive(Deserialize)]
pub struct ChangePasswordForm {
    current_password: SecretString,
    new_password: SecretString,
    new_password_check: SecretString,
}

pub async fn change_password_form(session: Session) -> Result<Html<String>, Response> {
    require_login(&session).await?;
    Ok(Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><title>Change password</title></head>
<body>
    {}
    <form action="/admin/password" method="post">
        <label>Current password
            <input type="password" name="current_password">
        </label>
        <label>New password
            <input type="password" name="new_password">
        </label>
        <label>Confirm new password
            <input type="password" name="new_password_check">
        </label>
        <button type="submit">Change password</button>
    </form>
</body>
</html>"#,
        flash_html(&session).await
    )))
}

pub async fn change_password_handler(
    State(state): State<ApplicationState>,
    session: Session,
    Form(form): Form<ChangePasswordForm>,
) -> Result<Response, Response> {
    let user_id = require_login(&session).await?;

    if form.new_password.expose_secret() != form.new_password_check.expose_secret() {
        set_flash(
            &session,
            "You entered two different new passwords - the field values must match.",
        )
        .await;
        return Ok(Redirect::to("/admin/password").into_response());
    }

    let username = sqlx::query_scalar::<_, String>(
        r#"
        SELECT username
        FROM users
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, "Failed to fetch username for password change");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;

    let credentials = Credentials {
        username,
        password: form.current_password,
    };
    if matches!(
        validate_credentials(credentials, &state.db_pool).await,
        Err(AuthError::InvalidCredentials(_))
    ) {
        set_flash(&session, "The current password is incorrect.").await;
        return Ok(Redirect::to("/admin/password").into_response());
    }

    change_password(user_id, form.new_password, &state.db_pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to change password");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;

    set_flash(&session, "Your password has been changed.").await;
    Ok(Redirect::to("/admin/password").into_response())
}

pub async fn require_login(session: &Session) -> Result<uuid::Uuid, Response> {
    TypedSession::new(session.clone())
        .get_user_id()
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to read session");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?
        .ok_or_else(|| Redirect::to("/login").into_response())
}

pub async fn set_flash(session: &Session, message: &str) {
    if let Err(error) = session.insert("flash", message.to_string()).await {
        tracing::error!(%error, "Failed to store flash message");
    }
}

pub async fn take_flash(session: &Session) -> Option<String> {
    session.remove("flash").await.ok().flatten()
}

async fn flash_html(session: &Session) -> String {
    take_flash(session)
        .await
        .map(|message| format!("<p><i>{message}</i></p>"))
        .unwrap_or_default()
}
