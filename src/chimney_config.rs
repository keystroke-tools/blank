use std::path::{Path, PathBuf};

use actix_web::{HttpRequest, HttpResponse, http::header, web};
use anyhow::Context;
use chimney::config::{Site, SiteBuilder, Sites};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;

use crate::{auth::require_session, error::ApiError, state::AppState};

#[derive(FromRow)]
struct ConfigRow {
    config_json: String,
    config_toml: String,
    origin: String,
    imported_hash: Option<String>,
    imported_commit: Option<String>,
    upstream_hash: Option<String>,
    updated_at: String,
}

#[derive(Serialize)]
struct ConfigurationResponse {
    config: Site,
    toml: String,
    origin: String,
    imported_hash: Option<String>,
    imported_commit: Option<String>,
    upstream_hash: Option<String>,
    upstream_changed: bool,
    updated_at: String,
}

#[derive(Deserialize)]
pub struct ConfigurationInput {
    toml: Option<String>,
    config: Option<Site>,
}

#[derive(FromRow)]
struct SiteSource {
    name: String,
    repository_url: String,
    branch: String,
    project_directory: String,
}

fn hash(input: &[u8]) -> String {
    Sha256::digest(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_site(mut site: Site, name: &str) -> Result<Site, ApiError> {
    site.name = name.to_owned();
    if site.root.is_empty()
        || Path::new(&site.root).is_absolute()
        || site.root.split('/').any(|part| part == "..")
    {
        return Err(ApiError::BadRequest(
            "Chimney root must be a relative path without '..'".into(),
        ));
    }
    if site.domain_names.is_empty() {
        return Err(ApiError::BadRequest(
            "at least one domain is required".into(),
        ));
    }
    if let Some(https) = &site.https_config {
        https
            .validate(name)
            .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    }
    let mut sites = Sites::default();
    sites
        .add(site.clone())
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    Ok(site)
}

fn parse_site(name: &str, input: &str) -> Result<Site, ApiError> {
    let site = Site::from_string(name.to_owned(), input)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    validate_site(site, name)
}

fn site_toml(site: &Site) -> Result<String, ApiError> {
    let mut value = toml::Value::try_from(site).map_err(|error| {
        ApiError::BadRequest(format!(
            "could not serialize Chimney configuration: {error}"
        ))
    })?;
    if let Some(table) = value.as_table_mut() {
        table.remove("name");
    }
    toml::to_string_pretty(&value).map_err(|error| {
        ApiError::BadRequest(format!(
            "could not serialize Chimney configuration: {error}"
        ))
    })
}

async fn site_source(state: &AppState, id: &str) -> Result<SiteSource, ApiError> {
    sqlx::query_as("SELECT name, repository_url, branch, project_directory FROM sites WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .context("failed to load site")?
        .ok_or_else(|| ApiError::NotFound("site not found".into()))
}

async fn generated_site(state: &AppState, id: &str, name: &str) -> Result<Site, ApiError> {
    let domains: Vec<String> =
        sqlx::query_scalar("SELECT domain FROM site_domains WHERE site_id = ? ORDER BY domain")
            .bind(id)
            .fetch_all(&state.db)
            .await
            .context("failed to load domains")?;
    if domains.is_empty() {
        return Err(ApiError::BadRequest(
            "add at least one domain before configuring Chimney".into(),
        ));
    }
    Ok(SiteBuilder::new(name)
        .domains(domains)
        .root(".")
        .default_index_file("index.html")
        .build())
}

async fn save(
    state: &AppState,
    id: &str,
    site: &Site,
    origin: &str,
    imported_hash: Option<&str>,
    imported_commit: Option<&str>,
) -> Result<(), ApiError> {
    let json = serde_json::to_string(site).context("failed to serialize Chimney configuration")?;
    let toml = site_toml(site)?;
    let mut tx = state
        .db
        .begin()
        .await
        .context("failed to save configuration")?;
    for domain in &site.domain_names {
        let conflict: Option<String> = sqlx::query_scalar(
            "SELECT site_id FROM site_domains WHERE domain = ? AND site_id != ?",
        )
        .bind(domain)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to check domain")?;
        if conflict.is_some() {
            return Err(ApiError::Conflict(format!(
                "domain is already used by another site: {domain}"
            )));
        }
    }
    sqlx::query("DELETE FROM site_domains WHERE site_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("failed to replace domains")?;
    for domain in &site.domain_names {
        sqlx::query("INSERT INTO site_domains (site_id, domain) VALUES (?, ?)")
            .bind(id)
            .bind(domain)
            .execute(&mut *tx)
            .await
            .context("failed to save domain")?;
    }
    sqlx::query("INSERT INTO site_chimney_configs (site_id, config_json, config_toml, origin, imported_hash, imported_commit, upstream_hash) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(site_id) DO UPDATE SET config_json=excluded.config_json, config_toml=excluded.config_toml, origin=excluded.origin, imported_hash=CASE WHEN excluded.origin='dashboard' THEN site_chimney_configs.imported_hash ELSE excluded.imported_hash END, imported_commit=CASE WHEN excluded.origin='dashboard' THEN site_chimney_configs.imported_commit ELSE excluded.imported_commit END, upstream_hash=CASE WHEN excluded.origin='dashboard' THEN site_chimney_configs.upstream_hash ELSE excluded.upstream_hash END, updated_at=CURRENT_TIMESTAMP")
        .bind(id).bind(json).bind(toml).bind(origin).bind(imported_hash).bind(imported_commit).bind(imported_hash).execute(&mut *tx).await.context("failed to persist Chimney configuration")?;
    tx.commit()
        .await
        .context("failed to commit Chimney configuration")?;
    state
        .chimney
        .reload(&state.db)
        .await
        .context("configuration saved but Chimney reload failed")?;
    Ok(())
}

async fn load(state: &AppState, id: &str) -> Result<ConfigurationResponse, ApiError> {
    let source = site_source(state, id).await?;
    let row = sqlx::query_as::<_, ConfigRow>("SELECT config_json, config_toml, origin, imported_hash, imported_commit, upstream_hash, updated_at FROM site_chimney_configs WHERE site_id = ?")
        .bind(id).fetch_optional(&state.db).await.context("failed to load configuration")?;
    let row = if let Some(row) = row {
        row
    } else {
        let site = generated_site(state, id, &source.name).await?;
        save(state, id, &site, "generated", None, None).await?;
        sqlx::query_as::<_, ConfigRow>("SELECT config_json, config_toml, origin, imported_hash, imported_commit, upstream_hash, updated_at FROM site_chimney_configs WHERE site_id = ?").bind(id).fetch_one(&state.db).await.context("failed to reload configuration")?
    };
    let mut config: Site = serde_json::from_str(&row.config_json)
        .context("stored Chimney configuration is invalid")?;
    config.name = source.name;
    let upstream_changed = row.imported_hash.is_some()
        && row.upstream_hash.is_some()
        && row.imported_hash != row.upstream_hash;
    Ok(ConfigurationResponse {
        config,
        toml: row.config_toml,
        origin: row.origin,
        imported_hash: row.imported_hash,
        imported_commit: row.imported_commit,
        upstream_hash: row.upstream_hash,
        upstream_changed,
        updated_at: row.updated_at,
    })
}

pub async fn get(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    Ok(HttpResponse::Ok().json(load(&state, &id).await?))
}

pub async fn update(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
    input: web::Json<ConfigurationInput>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let source = site_source(&state, &id).await?;
    let site = match (&input.toml, &input.config) {
        (Some(toml), None) => parse_site(&source.name, toml)?,
        (None, Some(config)) => validate_site(config.clone(), &source.name)?,
        _ => {
            return Err(ApiError::BadRequest(
                "provide exactly one of toml or config".into(),
            ));
        }
    };
    save(&state, &id, &site, "dashboard", None, None).await?;
    tracing::info!(site_id = %id, "Chimney configuration changed");
    Ok(HttpResponse::Ok().json(load(&state, &id).await?))
}

async fn repository_file(
    state: &AppState,
    id: &str,
) -> Result<(SiteSource, String, Option<Vec<u8>>), ApiError> {
    let source = site_source(state, id).await?;
    let token = crate::github::token_for_repository(state, &source.repository_url).await?;
    state
        .git
        .fetch_with_token(id, &source.repository_url, token.as_deref())
        .await
        .map_err(|error| ApiError::BadRequest(format!("repository fetch failed: {error}")))?;
    let commit = state
        .git
        .resolve_commit(id, &source.branch)
        .await
        .map_err(|error| ApiError::BadRequest(format!("branch resolution failed: {error}")))?;
    let deployment_id = format!("config-{}", uuid::Uuid::new_v4());
    let worktree = state
        .git
        .create_worktree(id, &deployment_id, &commit.sha)
        .await
        .context("failed to create configuration worktree")?;
    let result = read_config_file(worktree.path(), &source.project_directory).await;
    worktree
        .remove()
        .await
        .context("failed to clean configuration worktree")?;
    Ok((source, commit.sha, result?))
}

async fn read_config_file(
    worktree: &Path,
    project_directory: &str,
) -> Result<Option<Vec<u8>>, ApiError> {
    let root = tokio::fs::canonicalize(worktree)
        .await
        .context("failed to resolve worktree")?;
    let project = tokio::fs::canonicalize(worktree.join(project_directory))
        .await
        .map_err(|_| ApiError::BadRequest("project directory does not exist".into()))?;
    if !project.starts_with(&root) {
        return Err(ApiError::BadRequest(
            "project directory escapes the repository".into(),
        ));
    }
    let path: PathBuf = project.join("chimney.toml");
    let resolved = match tokio::fs::canonicalize(&path).await {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ApiError::Internal(error.into())),
    };
    if !resolved.starts_with(&project) {
        return Err(ApiError::BadRequest(
            "chimney.toml symlink escapes the project directory".into(),
        ));
    }
    tokio::fs::read(resolved)
        .await
        .map(Some)
        .map_err(|error| ApiError::Internal(error.into()))
}

pub async fn import(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let (source, commit, file) = repository_file(&state, &id).await?;
    let (site, imported_hash, origin) = if let Some(file) = file {
        let input = std::str::from_utf8(&file)
            .map_err(|_| ApiError::BadRequest("chimney.toml must be UTF-8".into()))?;
        (
            parse_site(&source.name, input)?,
            Some(hash(&file)),
            "repository",
        )
    } else {
        (
            generated_site(&state, &id, &source.name).await?,
            None,
            "generated",
        )
    };
    save(
        &state,
        &id,
        &site,
        origin,
        imported_hash.as_deref(),
        imported_hash.as_ref().map(|_| commit.as_str()),
    )
    .await?;
    tracing::info!(site_id = %id, origin, "Chimney configuration imported");
    Ok(HttpResponse::Ok().json(load(&state, &id).await?))
}

pub async fn check_upstream(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let (_, _, file) = repository_file(&state, &id).await?;
    let upstream_hash = file.as_deref().map(hash);
    sqlx::query("UPDATE site_chimney_configs SET upstream_hash = ? WHERE site_id = ?")
        .bind(upstream_hash)
        .bind(id.as_str())
        .execute(&state.db)
        .await
        .context("failed to store upstream hash")?;
    Ok(HttpResponse::Ok().json(load(&state, &id).await?))
}

pub async fn renew_certificates(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let _ = site_source(&state, &id).await?;
    state
        .chimney
        .reload(&state.db)
        .await
        .context("failed to reload Chimney certificates")?;
    tracing::info!(site_id = %id, "requested certificate renewal");
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "reload_requested" })))
}

pub async fn export(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let config = load(&state, &id).await?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "application/toml; charset=utf-8"))
        .insert_header((
            header::CONTENT_DISPOSITION,
            "attachment; filename=chimney.toml",
        ))
        .body(config.toml))
}

pub fn routes(config: &mut web::ServiceConfig) {
    config
        .route("/sites/{id}/configuration", web::get().to(get))
        .route("/sites/{id}/configuration", web::put().to(update))
        .route("/sites/{id}/configuration/import", web::post().to(import))
        .route(
            "/sites/{id}/configuration/check-upstream",
            web::post().to(check_upstream),
        )
        .route(
            "/sites/{id}/certificates/renew",
            web::post().to(renew_certificates),
        )
        .route("/sites/{id}/configuration/export", web::get().to(export));
}

pub async fn ensure_default(state: &AppState, id: &str) -> Result<(), ApiError> {
    let _ = load(state, id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_and_canonicalizes_site_toml() {
        let site = parse_site(
            "docs",
            "root = '.'\ndomain_names = ['docs.example.com']\nfallback_file = 'index.html'\n",
        )
        .unwrap();
        let output = site_toml(&site).unwrap();
        assert!(!output.contains("name ="));
        assert!(output.contains("docs.example.com"));
    }
    #[test]
    fn detects_content_hash_changes() {
        assert_ne!(hash(b"root='.'"), hash(b"root='dist'"));
    }
    #[test]
    fn rejects_escaping_root() {
        assert!(
            parse_site(
                "docs",
                "root = '../secret'\ndomain_names = ['docs.example.com']"
            )
            .is_err()
        );
    }
}
