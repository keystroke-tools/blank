use actix_web::{HttpRequest, HttpResponse, http::header, web};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};

use crate::{auth::require_session, error::ApiError, state::AppState};

#[derive(Serialize)]
struct Status {
    connected: bool,
    app_slug: Option<String>,
    install_url: Option<String>,
    manifest_url: Option<String>,
    manifest: Option<String>,
}

#[derive(sqlx::FromRow)]
struct AppConfig {
    app_id: i64,
    app_slug: String,
    webhook_secret: String,
    private_key_pem: String,
}

#[derive(Deserialize)]
struct ManifestCallback {
    code: String,
    state: Option<String>,
}

#[derive(Deserialize)]
struct ConnectQuery {
    return_to: Option<String>,
}

#[derive(Deserialize)]
struct ManifestResponse {
    id: i64,
    slug: String,
    client_id: String,
    client_secret: String,
    webhook_secret: String,
    pem: String,
}

#[derive(Serialize)]
struct Claims {
    iat: i64,
    exp: i64,
    iss: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct RepositoryPage {
    repositories: Vec<Repository>,
}

#[derive(Deserialize, Serialize)]
pub struct Repository {
    id: i64,
    full_name: String,
    clone_url: String,
    private: bool,
    default_branch: String,
}

async fn status(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let app = load_app(&state).await?;
    let (manifest_url, manifest) = manifest(&state)?;
    Ok(HttpResponse::Ok().json(Status {
        connected: app.is_some(),
        app_slug: app.as_ref().map(|app| app.app_slug.clone()),
        install_url: app
            .map(|app| format!("https://github.com/apps/{}/installations/new", app.app_slug)),
        manifest_url: Some(manifest_url),
        manifest: Some(manifest),
    }))
}

async fn connect(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ConnectQuery>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let (mut action, manifest) = manifest(&state)?;
    let return_to = safe_return_path(query.return_to.as_deref());
    action.push_str("?state=");
    action.push_str(&URL_SAFE_NO_PAD.encode(return_to));
    let escaped = manifest
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;");
    Ok(HttpResponse::Ok().content_type("text/html; charset=utf-8").body(format!(r#"<!doctype html><title>Connect GitHub</title><form id="connect" method="post" action="{action}"><input type="hidden" name="manifest" value="{escaped}"></form><script>document.getElementById('connect').submit()</script>"#)))
}

fn manifest(state: &AppState) -> Result<(String, String), ApiError> {
    let public_url = state.config.public_url.as_deref().ok_or_else(|| {
        ApiError::Conflict("BLANK_PUBLIC_URL must be configured before connecting GitHub".into())
    })?;
    Ok((
        "https://github.com/settings/apps/new".into(),
        manifest_value(public_url).to_string(),
    ))
}

fn manifest_value(public_url: &str) -> serde_json::Value {
    serde_json::json!({
        "name": "Blank Deploy",
        "url": public_url,
        "redirect_url": format!("{public_url}/api/github/manifest/callback"),
        "hook_attributes": { "url": format!("{public_url}/api/webhooks/github"), "active": true },
        "public": false,
        "default_permissions": { "contents": "read" },
        "default_events": ["push"]
    })
}

fn safe_return_path(value: Option<&str>) -> &str {
    value
        .filter(|path| {
            (*path == "/dashboard" || path.starts_with("/sites/"))
                && !path.starts_with("//")
                && !path.contains(['\\', '\r', '\n'])
        })
        .unwrap_or("/dashboard")
}

async fn callback(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ManifestCallback>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let response = reqwest::Client::new()
        .post(format!(
            "https://api.github.com/app-manifests/{}/conversions",
            query.code
        ))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "Blank")
        .send()
        .await
        .map_err(anyhow::Error::from)?;
    if !response.status().is_success() {
        return Err(ApiError::BadRequest(
            "GitHub App registration could not be completed".into(),
        ));
    }
    let app: ManifestResponse = response.json().await.map_err(anyhow::Error::from)?;
    sqlx::query("INSERT INTO github_app_config (id, app_id, app_slug, client_id, client_secret, webhook_secret, private_key_pem) VALUES (1, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET app_id=excluded.app_id, app_slug=excluded.app_slug, client_id=excluded.client_id, client_secret=excluded.client_secret, webhook_secret=excluded.webhook_secret, private_key_pem=excluded.private_key_pem, updated_at=CURRENT_TIMESTAMP")
        .bind(app.id).bind(app.slug).bind(app.client_id).bind(app.client_secret).bind(app.webhook_secret).bind(app.pem).execute(&state.db).await.map_err(anyhow::Error::from)?;
    let return_to = query
        .state
        .as_deref()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .and_then(|value| String::from_utf8(value).ok());
    Ok(HttpResponse::SeeOther()
        .insert_header((
            header::LOCATION,
            safe_return_path(return_to.as_deref()).to_owned(),
        ))
        .finish())
}

async fn repositories(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let installations: Vec<i64> = sqlx::query_scalar(
        "SELECT installation_id FROM github_installations ORDER BY account_login",
    )
    .fetch_all(&state.db)
    .await
    .map_err(anyhow::Error::from)?;
    let mut repositories = Vec::new();
    for installation in installations {
        let token = installation_token(&state, installation).await?;
        let response = reqwest::Client::new()
            .get("https://api.github.com/installation/repositories?per_page=100")
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "Blank")
            .send()
            .await
            .map_err(anyhow::Error::from)?;
        if response.status().is_success() {
            repositories.extend(
                response
                    .json::<RepositoryPage>()
                    .await
                    .map_err(anyhow::Error::from)?
                    .repositories,
            );
        }
    }
    repositories.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    Ok(HttpResponse::Ok().json(repositories))
}

pub async fn webhook_secret(state: &AppState) -> Result<Option<String>, ApiError> {
    Ok(load_app(state).await?.map(|app| app.webhook_secret))
}

async fn load_app(state: &AppState) -> Result<Option<AppConfig>, ApiError> {
    sqlx::query_as("SELECT app_id, app_slug, webhook_secret, private_key_pem FROM github_app_config WHERE id=1")
        .fetch_optional(&state.db).await.map_err(anyhow::Error::from).map_err(Into::into)
}

async fn installation_token(state: &AppState, installation_id: i64) -> Result<String, ApiError> {
    let app = load_app(state)
        .await?
        .ok_or_else(|| ApiError::Conflict("GitHub is not connected".into()))?;
    let now = Utc::now().timestamp();
    let jwt = encode(
        &Header::new(Algorithm::RS256),
        &Claims {
            iat: now - 60,
            exp: now + 540,
            iss: app.app_id.to_string(),
        },
        &EncodingKey::from_rsa_pem(app.private_key_pem.as_bytes()).map_err(anyhow::Error::from)?,
    )
    .map_err(anyhow::Error::from)?;
    let response = reqwest::Client::new()
        .post(format!(
            "https://api.github.com/app/installations/{installation_id}/access_tokens"
        ))
        .bearer_auth(jwt)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "Blank")
        .send()
        .await
        .map_err(anyhow::Error::from)?;
    if !response.status().is_success() {
        return Err(ApiError::BadRequest(
            "GitHub installation token could not be created".into(),
        ));
    }
    Ok(response
        .json::<TokenResponse>()
        .await
        .map_err(anyhow::Error::from)?
        .token)
}

pub async fn token_for_repository(
    state: &AppState,
    repository_url: &str,
) -> Result<Option<String>, ApiError> {
    if load_app(state).await?.is_none() || !repository_url.contains("github.com") {
        return Ok(None);
    }
    let target = repository_url
        .trim_end_matches(".git")
        .rsplit_once("github.com")
        .map(|(_, value)| value.trim_start_matches(['/', ':']))
        .unwrap_or_default();
    let installations: Vec<i64> =
        sqlx::query_scalar("SELECT installation_id FROM github_installations")
            .fetch_all(&state.db)
            .await
            .map_err(anyhow::Error::from)?;
    for installation in installations {
        let token = installation_token(state, installation).await?;
        let response = reqwest::Client::new()
            .get("https://api.github.com/installation/repositories?per_page=100")
            .bearer_auth(&token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "Blank")
            .send()
            .await
            .map_err(anyhow::Error::from)?;
        if response.status().is_success()
            && response
                .json::<RepositoryPage>()
                .await
                .map_err(anyhow::Error::from)?
                .repositories
                .iter()
                .any(|repo| repo.full_name.eq_ignore_ascii_case(target))
        {
            return Ok(Some(token));
        }
    }
    Ok(None)
}

pub fn routes(config: &mut web::ServiceConfig) {
    config
        .route("/github/status", web::get().to(status))
        .route("/github/connect", web::get().to(connect))
        .route("/github/manifest/callback", web::get().to(callback))
        .route("/github/repositories", web::get().to(repositories));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_github_registration_schema() {
        let value = manifest_value("https://blank.example.com");

        assert_eq!(
            value,
            serde_json::json!({
                "name": "Blank Deploy",
                "url": "https://blank.example.com",
                "redirect_url": "https://blank.example.com/api/github/manifest/callback",
                "hook_attributes": {
                    "url": "https://blank.example.com/api/webhooks/github",
                    "active": true
                },
                "public": false,
                "default_permissions": { "contents": "read" },
                "default_events": ["push"]
            })
        );
    }

    #[test]
    fn github_return_paths_stay_inside_the_admin_app() {
        assert_eq!(safe_return_path(Some("/dashboard")), "/dashboard");
        assert_eq!(
            safe_return_path(Some("/sites/site-id/settings")),
            "/sites/site-id/settings"
        );
        assert_eq!(safe_return_path(Some("//example.com")), "/dashboard");
        assert_eq!(safe_return_path(Some("https://example.com")), "/dashboard");
    }
}
