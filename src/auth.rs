use actix_web::{
    HttpRequest, HttpResponse,
    cookie::{Cookie, SameSite, time::Duration},
    web,
};
use anyhow::Context;
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::{error::ApiError, state::AppState};

const SESSION_COOKIE: &str = "blank_session";

#[derive(Serialize)]
pub struct AuthStatus {
    setup_required: bool,
    authenticated: bool,
    identifier: Option<String>,
    csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct Credentials {
    identifier: String,
    password: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Administrator {
    id: i64,
    identifier: String,
    created_at: String,
    is_current: bool,
}

pub(crate) struct Session {
    pub administrator_id: i64,
    pub identifier: String,
    pub csrf_token: String,
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn validate_credentials(input: &Credentials) -> Result<(), ApiError> {
    let identifier = input.identifier.trim();
    if identifier.len() < 3 || identifier.len() > 254 || identifier.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "identifier must be between 3 and 254 characters".into(),
        ));
    }
    if input.password.len() < 12 {
        return Err(ApiError::BadRequest(
            "password must be at least 12 characters".into(),
        ));
    }
    Ok(())
}

async fn hash_password(password: String) -> Result<String, ApiError> {
    web::block(move || {
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|hash| hash.to_string())
    })
    .await
    .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?
    .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))
}

async fn current_session(req: &HttpRequest, db: &SqlitePool) -> Result<Option<Session>, ApiError> {
    let Some(cookie) = req.cookie(SESSION_COOKIE) else {
        return Ok(None);
    };
    let row = sqlx::query("SELECT a.id AS administrator_id, a.identifier, s.csrf_token FROM sessions s JOIN administrators a ON a.id = s.administrator_id WHERE s.token_hash = ? AND s.expires_at > CURRENT_TIMESTAMP")
        .bind(token_hash(cookie.value())).fetch_optional(db).await
        .context("failed to load session")?;
    Ok(row.map(|row| Session {
        administrator_id: row.get("administrator_id"),
        identifier: row.get("identifier"),
        csrf_token: row.get("csrf_token"),
    }))
}

pub(crate) async fn require_session(
    req: &HttpRequest,
    db: &SqlitePool,
    require_csrf: bool,
) -> Result<Session, ApiError> {
    let session = current_session(req, db)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if require_csrf
        && req
            .headers()
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            != Some(session.csrf_token.as_str())
    {
        return Err(ApiError::Forbidden);
    }
    Ok(session)
}

fn session_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build(SESSION_COOKIE, token)
        .path("/")
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .max_age(Duration::days(7))
        .finish()
}

async fn create_session(
    db: &SqlitePool,
    administrator_id: i64,
) -> Result<(String, String), ApiError> {
    let token = random_token();
    let csrf = random_token();
    let expires = Utc::now() + ChronoDuration::days(7);
    sqlx::query("INSERT INTO sessions (token_hash, administrator_id, csrf_token, expires_at) VALUES (?, ?, ?, ?)")
        .bind(token_hash(&token)).bind(administrator_id).bind(&csrf).bind(expires).execute(db).await
        .context("failed to create session")?;
    Ok((token, csrf))
}

pub async fn status(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM administrators")
        .fetch_one(&state.db)
        .await
        .context("failed to inspect setup state")?;
    let session = current_session(&req, &state.db).await?;
    Ok(HttpResponse::Ok().json(AuthStatus {
        setup_required: admin_count == 0,
        authenticated: session.is_some(),
        identifier: session.as_ref().map(|s| s.identifier.clone()),
        csrf_token: session.map(|s| s.csrf_token),
    }))
}

pub async fn setup(
    state: web::Data<AppState>,
    input: web::Json<Credentials>,
) -> Result<HttpResponse, ApiError> {
    validate_credentials(&input)?;
    let hash = hash_password(input.password.clone()).await?;
    let inserted = sqlx::query("INSERT INTO administrators (id, identifier, password_hash) VALUES (1, ?, ?) ON CONFLICT DO NOTHING")
        .bind(input.identifier.trim()).bind(hash).execute(&state.db).await.context("failed to create administrator")?;
    if inserted.rows_affected() == 0 {
        return Err(ApiError::Conflict(
            "setup has already been completed".into(),
        ));
    }
    let (token, csrf_token) = create_session(&state.db, 1).await?;
    Ok(HttpResponse::Created()
        .cookie(session_cookie(token, state.config.secure_cookies))
        .json(AuthStatus {
            setup_required: false,
            authenticated: true,
            identifier: Some(input.identifier.trim().into()),
            csrf_token: Some(csrf_token),
        }))
}

pub async fn login(
    state: web::Data<AppState>,
    input: web::Json<Credentials>,
) -> Result<HttpResponse, ApiError> {
    let Some(row) =
        sqlx::query("SELECT id, identifier, password_hash FROM administrators WHERE identifier = ? COLLATE NOCASE")
            .bind(input.identifier.trim())
            .fetch_optional(&state.db)
            .await
            .context("failed to load administrator")?
    else {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        return Err(ApiError::Unauthorized);
    };
    let hash: String = row.get("password_hash");
    let password = input.password.clone();
    let valid = web::block(move || {
        PasswordHash::new(&hash).ok().is_some_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?;
    if !valid {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        return Err(ApiError::Unauthorized);
    }
    let administrator_id: i64 = row.get("id");
    let (token, csrf_token) = create_session(&state.db, administrator_id).await?;
    Ok(HttpResponse::Ok()
        .cookie(session_cookie(token, state.config.secure_cookies))
        .json(AuthStatus {
            setup_required: false,
            authenticated: true,
            identifier: Some(row.get("identifier")),
            csrf_token: Some(csrf_token),
        }))
}

pub async fn list_administrators(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    let session = require_session(&req, &state.db, false).await?;
    let administrators = sqlx::query_as::<_, Administrator>(
        "SELECT id, identifier, created_at, id = ? AS is_current FROM administrators ORDER BY identifier COLLATE NOCASE",
    )
    .bind(session.administrator_id)
    .fetch_all(&state.db)
    .await
    .context("failed to list administrators")?;
    Ok(HttpResponse::Ok().json(administrators))
}

pub async fn create_administrator(
    req: HttpRequest,
    state: web::Data<AppState>,
    input: web::Json<Credentials>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    validate_credentials(&input)?;
    let identifier = input.identifier.trim();
    let hash = hash_password(input.password.clone()).await?;
    let result =
        sqlx::query("INSERT INTO administrators (identifier, password_hash) VALUES (?, ?)")
            .bind(identifier)
            .bind(hash)
            .execute(&state.db)
            .await;
    let result = match result {
        Ok(result) => result,
        Err(error)
            if error
                .as_database_error()
                .is_some_and(|error| error.is_unique_violation()) =>
        {
            return Err(ApiError::Conflict(
                "an administrator with that identifier already exists".into(),
            ));
        }
        Err(error) => {
            return Err(ApiError::Internal(
                anyhow::Error::new(error).context("failed to create administrator"),
            ));
        }
    };
    let administrator = sqlx::query_as::<_, Administrator>(
        "SELECT id, identifier, created_at, 0 AS is_current FROM administrators WHERE id = ?",
    )
    .bind(result.last_insert_rowid())
    .fetch_one(&state.db)
    .await
    .context("failed to load created administrator")?;
    Ok(HttpResponse::Created().json(administrator))
}

pub async fn logout(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    if let Some(cookie) = req.cookie(SESSION_COOKIE) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash(cookie.value()))
            .execute(&state.db)
            .await
            .context("failed to delete session")?;
    }
    let expired = Cookie::build(SESSION_COOKIE, "")
        .path("/")
        .max_age(Duration::ZERO)
        .finish();
    Ok(HttpResponse::NoContent().cookie(expired).finish())
}

pub fn routes(config: &mut web::ServiceConfig) {
    config.service(
        web::scope("/auth")
            .route("/status", web::get().to(status))
            .route("/setup", web::post().to(setup))
            .route("/login", web::post().to(login))
            .route("/logout", web::post().to(logout))
            .route("/administrators", web::get().to(list_administrators))
            .route("/administrators", web::post().to(create_administrator)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_require_a_useful_identifier() {
        let input = Credentials {
            identifier: "ab".into(),
            password: "long-enough-password".into(),
        };
        assert!(matches!(
            validate_credentials(&input),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn credentials_require_a_long_password() {
        let input = Credentials {
            identifier: "admin".into(),
            password: "too-short".into(),
        };
        assert!(matches!(
            validate_credentials(&input),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn session_tokens_are_stored_as_fixed_size_hashes() {
        assert_eq!(token_hash("secret").len(), 32);
        assert_ne!(token_hash("secret"), b"secret");
    }
}
