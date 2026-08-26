use actix_web::{HttpRequest, HttpResponse, web};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, Transaction};
use uuid::Uuid;

use crate::{
    auth::require_session,
    dns::valid_domain,
    error::ApiError,
    git::{validate_branch, validate_repository_url},
    state::AppState,
};

#[derive(Debug, Serialize, FromRow)]
struct SiteRow {
    id: String,
    name: String,
    repository_url: String,
    branch: String,
    project_directory: String,
    mise_tools: String,
    detected_framework: Option<String>,
    install_command: Option<String>,
    build_command: Option<String>,
    publish_directory: String,
    build_enabled: bool,
    auto_deploy: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct Site {
    #[serde(flatten)]
    row: SiteRow,
    domains: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SiteInput {
    name: String,
    repository_url: String,
    #[serde(default = "default_branch")]
    branch: String,
    #[serde(default = "default_project_directory")]
    project_directory: String,
    #[serde(default)]
    mise_tools: String,
    #[serde(default)]
    detected_framework: Option<String>,
    install_command: Option<String>,
    build_command: Option<String>,
    #[serde(default = "default_publish_directory")]
    publish_directory: String,
    #[serde(default = "default_true")]
    build_enabled: bool,
    #[serde(default)]
    auto_deploy: bool,
    #[serde(default)]
    domains: Vec<String>,
}

fn default_branch() -> String {
    "main".into()
}
fn default_project_directory() -> String {
    ".".into()
}
fn default_publish_directory() -> String {
    "dist".into()
}
fn default_true() -> bool {
    true
}

fn clean_optional(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn normalized_domains(domains: &[String]) -> Result<Vec<String>, ApiError> {
    if domains.len() > 20 {
        return Err(ApiError::BadRequest(
            "a site may have at most 20 domains".into(),
        ));
    }
    let mut result = Vec::new();
    for domain in domains {
        let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
        if domain.is_empty() {
            continue;
        }
        if !valid_domain(&domain) {
            return Err(ApiError::BadRequest(format!("invalid domain: {domain}")));
        }
        if !result.contains(&domain) {
            result.push(domain);
        }
    }
    Ok(result)
}

pub(crate) fn normalized_mise_tools(value: &str) -> Result<String, ApiError> {
    let tools: Vec<_> = value
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|tool| !tool.is_empty())
        .collect();
    if tools.len() > 20
        || tools.iter().any(|tool| {
            tool.len() > 200
                || tool.starts_with('-')
                || tool.chars().any(|character| {
                    character.is_control() || matches!(character, '\'' | '"' | '`' | '$' | ';')
                })
        })
    {
        return Err(ApiError::BadRequest(
            "dependencies must contain at most 20 valid specifications".into(),
        ));
    }
    Ok(tools.join("\n"))
}

fn validate(input: &SiteInput) -> Result<Vec<String>, ApiError> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 100 || name.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "site name must be 1–100 characters without control characters".into(),
        ));
    }
    if input.repository_url.len() > 2048 {
        return Err(ApiError::BadRequest("repository URL is too long".into()));
    }
    for (label, command) in [
        ("install command", &input.install_command),
        ("build command", &input.build_command),
    ] {
        if let Some(command) = command.as_deref() {
            if command.len() > 4096 || command.chars().any(char::is_control) {
                return Err(ApiError::BadRequest(format!(
                    "{label} is too long or contains control characters"
                )));
            }
        }
    }
    validate_repository_url(input.repository_url.trim())
        .map_err(|_| ApiError::BadRequest("invalid repository URL".into()))?;
    validate_branch(input.branch.trim())
        .map_err(|_| ApiError::BadRequest("invalid branch name".into()))?;
    normalized_mise_tools(&input.mise_tools)?;
    if input
        .detected_framework
        .as_ref()
        .is_some_and(|framework| framework.len() > 100 || framework.chars().any(char::is_control))
    {
        return Err(ApiError::BadRequest("invalid detected framework".into()));
    }
    for (label, path) in [
        ("project directory", input.project_directory.trim()),
        ("publish directory", input.publish_directory.trim()),
    ] {
        if path.is_empty()
            || path.len() > 512
            || path.starts_with('/')
            || path.contains('\\')
            || path.split('/').any(|part| part == "..")
        {
            return Err(ApiError::BadRequest(format!(
                "{label} must be a relative path without '..'"
            )));
        }
    }
    normalized_domains(&input.domains)
}

async fn domains_for(state: &AppState, site_id: &str) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar("SELECT domain FROM site_domains WHERE site_id = ? ORDER BY domain")
        .bind(site_id)
        .fetch_all(&state.db)
        .await
        .context("failed to load site domains")
        .map_err(Into::into)
}

async fn load_site(state: &AppState, id: &str) -> Result<Site, ApiError> {
    let row = sqlx::query_as::<_, SiteRow>("SELECT id, name, repository_url, branch, project_directory, mise_tools, detected_framework, install_command, build_command, publish_directory, build_enabled, auto_deploy, created_at, updated_at FROM sites WHERE id = ?")
        .bind(id).fetch_optional(&state.db).await.context("failed to load site")?
        .ok_or_else(|| ApiError::NotFound("site not found".into()))?;
    let domains = domains_for(state, id).await?;
    Ok(Site { row, domains })
}

async fn replace_domains(
    tx: &mut Transaction<'_, Sqlite>,
    site_id: &str,
    domains: &[String],
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM site_domains WHERE site_id = ?")
        .bind(site_id)
        .execute(&mut **tx)
        .await
        .context("failed to replace domains")?;
    for domain in domains {
        let result = sqlx::query("INSERT INTO site_domains (site_id, domain) VALUES (?, ?)")
            .bind(site_id)
            .bind(domain)
            .execute(&mut **tx)
            .await;
        if let Err(error) = result {
            if error
                .as_database_error()
                .is_some_and(|error| error.is_unique_violation())
            {
                return Err(ApiError::Conflict(format!(
                    "domain is already used by another site: {domain}"
                )));
            }
            return Err(ApiError::Internal(
                anyhow::Error::new(error).context("failed to save domain"),
            ));
        }
    }
    Ok(())
}

pub async fn list(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let rows = sqlx::query_as::<_, SiteRow>("SELECT id, name, repository_url, branch, project_directory, mise_tools, detected_framework, install_command, build_command, publish_directory, build_enabled, auto_deploy, created_at, updated_at FROM sites ORDER BY name")
        .fetch_all(&state.db).await.context("failed to list sites")?;
    let mut sites = Vec::with_capacity(rows.len());
    for row in rows {
        let domains = domains_for(&state, &row.id).await?;
        sites.push(Site { row, domains });
    }
    Ok(HttpResponse::Ok().json(sites))
}

pub async fn get(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    Ok(HttpResponse::Ok().json(load_site(&state, &id).await?))
}

pub async fn create(
    req: HttpRequest,
    state: web::Data<AppState>,
    input: web::Json<SiteInput>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let domains = validate(&input)?;
    let id = Uuid::new_v4().to_string();
    let mut tx = state
        .db
        .begin()
        .await
        .context("failed to begin site creation")?;
    let result = sqlx::query("INSERT INTO sites (id, name, repository_url, branch, project_directory, mise_tools, detected_framework, install_command, build_command, publish_directory, build_enabled, auto_deploy) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&id).bind(input.name.trim()).bind(input.repository_url.trim()).bind(input.branch.trim())
        .bind(input.project_directory.trim()).bind(normalized_mise_tools(&input.mise_tools)?).bind(clean_optional(&input.detected_framework)).bind(clean_optional(&input.install_command)).bind(clean_optional(&input.build_command))
        .bind(input.publish_directory.trim()).bind(input.build_enabled).bind(input.auto_deploy).execute(&mut *tx).await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|error| error.is_unique_violation())
        {
            return Err(ApiError::Conflict(
                "a site with that name already exists".into(),
            ));
        }
        return Err(ApiError::Internal(
            anyhow::Error::new(error).context("failed to create site"),
        ));
    }
    replace_domains(&mut tx, &id, &domains).await?;
    tx.commit()
        .await
        .context("failed to commit site creation")?;
    crate::chimney_config::ensure_default(&state, &id).await?;
    Ok(HttpResponse::Created().json(load_site(&state, &id).await?))
}

pub async fn update(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
    input: web::Json<SiteInput>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let domains = validate(&input)?;
    let mut tx = state
        .db
        .begin()
        .await
        .context("failed to begin site update")?;
    let result = sqlx::query("UPDATE sites SET name = ?, repository_url = ?, branch = ?, project_directory = ?, mise_tools = ?, detected_framework = ?, install_command = ?, build_command = ?, publish_directory = ?, build_enabled = ?, auto_deploy = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(input.name.trim()).bind(input.repository_url.trim()).bind(input.branch.trim()).bind(input.project_directory.trim())
        .bind(normalized_mise_tools(&input.mise_tools)?).bind(clean_optional(&input.detected_framework)).bind(clean_optional(&input.install_command)).bind(clean_optional(&input.build_command)).bind(input.publish_directory.trim())
        .bind(input.build_enabled).bind(input.auto_deploy).bind(id.as_str()).execute(&mut *tx).await;
    let result = match result {
        Ok(result) => result,
        Err(error)
            if error
                .as_database_error()
                .is_some_and(|error| error.is_unique_violation()) =>
        {
            return Err(ApiError::Conflict(
                "a site with that name already exists".into(),
            ));
        }
        Err(error) => {
            return Err(ApiError::Internal(
                anyhow::Error::new(error).context("failed to update site"),
            ));
        }
    };
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("site not found".into()));
    }
    replace_domains(&mut tx, &id, &domains).await?;
    tx.commit().await.context("failed to commit site update")?;
    state
        .chimney
        .reload(&state.db)
        .await
        .context("site updated but Chimney reload failed")?;
    Ok(HttpResponse::Ok().json(load_site(&state, &id).await?))
}

pub async fn delete(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let result = sqlx::query("DELETE FROM sites WHERE id = ?")
        .bind(id.as_str())
        .execute(&state.db)
        .await
        .context("failed to delete site")?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("site not found".into()));
    }
    if let Err(error) = state.git.delete_site_data(&id).await {
        tracing::warn!(site_id = %id, ?error, "failed to clean up site Git data");
    }
    state
        .chimney
        .reload(&state.db)
        .await
        .context("site deleted but Chimney reload failed")?;
    tracing::info!(site_id = %id, "site deleted");
    Ok(HttpResponse::NoContent().finish())
}

pub fn routes(config: &mut web::ServiceConfig) {
    config.service(
        web::scope("/sites")
            .route("", web::get().to(list))
            .route("", web::post().to(create))
            .route("/{id}", web::get().to(get))
            .route("/{id}", web::put().to(update))
            .route("/{id}", web::delete().to(delete)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(project: &str, publish: &str) -> SiteInput {
        SiteInput {
            name: "Docs".into(),
            repository_url: "https://example.com/docs.git".into(),
            branch: "main".into(),
            project_directory: project.into(),
            mise_tools: String::new(),
            detected_framework: None,
            install_command: None,
            build_command: None,
            publish_directory: publish.into(),
            build_enabled: true,
            auto_deploy: false,
            domains: vec![],
        }
    }
    #[test]
    fn rejects_project_traversal() {
        assert!(validate(&input("../api", "dist")).is_err());
    }
    #[test]
    fn rejects_publish_traversal() {
        assert!(validate(&input(".", "../secret")).is_err());
    }
    #[test]
    fn normalizes_and_deduplicates_domains() {
        let mut value = input(".", "dist");
        value.domains = vec!["Docs.Example.com.".into(), "docs.example.com".into()];
        assert_eq!(validate(&value).unwrap(), vec!["docs.example.com"]);
    }

    #[test]
    fn rejects_malformed_site_fields() {
        let mut value = input(".", "dist");
        value.domains = vec!["bad_domain.example.com".into()];
        assert!(validate(&value).is_err());
        value.domains.clear();
        value.branch = "../main".into();
        assert!(validate(&value).is_err());
        value.branch = "main".into();
        value.name = "\n".into();
        assert!(validate(&value).is_err());
    }
}
