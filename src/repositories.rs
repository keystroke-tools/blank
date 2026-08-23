use actix_web::{HttpRequest, HttpResponse, web};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{
    auth::require_session,
    error::ApiError,
    git::{CommitMetadata, RemoteInspection},
    state::AppState,
};

#[derive(Deserialize)]
pub struct InspectInput {
    repository_url: String,
}

#[derive(Serialize)]
struct RefreshResult {
    inspection: RemoteInspection,
    commit: CommitMetadata,
}

#[derive(FromRow)]
struct SiteRepository {
    repository_url: String,
    branch: String,
}

#[derive(Deserialize)]
struct TreeQuery {
    #[serde(default)]
    path: String,
    branch: Option<String>,
}

#[derive(Serialize)]
struct DeployKey {
    public_key: Option<String>,
}

async fn ensure_site(state: &AppState, id: &str) -> Result<(), ApiError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sites WHERE id = ?)")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .context("failed to verify site")?;
    if !exists {
        return Err(ApiError::NotFound("site not found".into()));
    }
    Ok(())
}

pub async fn inspect(
    req: HttpRequest,
    state: web::Data<AppState>,
    input: web::Json<InspectInput>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let inspection = state
        .git
        .inspect_remote(input.repository_url.trim())
        .await
        .map_err(|error| ApiError::BadRequest(format!("repository inspection failed: {error}")))?;
    Ok(HttpResponse::Ok().json(inspection))
}

pub async fn refresh(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let site = sqlx::query_as::<_, SiteRepository>(
        "SELECT repository_url, branch FROM sites WHERE id = ?",
    )
    .bind(id.as_str())
    .fetch_optional(&state.db)
    .await
    .context("failed to load site repository")?
    .ok_or_else(|| ApiError::NotFound("site not found".into()))?;
    state
        .git
        .fetch(&id, &site.repository_url)
        .await
        .map_err(|error| ApiError::BadRequest(format!("repository fetch failed: {error}")))?;
    let commit = state
        .git
        .resolve_commit(&id, &site.branch)
        .await
        .map_err(|error| ApiError::BadRequest(format!("branch resolution failed: {error}")))?;
    let branches = state
        .git
        .cached_branches(&id)
        .await
        .context("failed to read cached branches")?;
    let inspection = RemoteInspection {
        default_branch: Some(site.branch),
        branches,
    };
    tracing::info!(site_id = %id, commit = %commit.sha, "repository refreshed");
    Ok(HttpResponse::Ok().json(RefreshResult { inspection, commit }))
}

async fn tree(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
    query: web::Query<TreeQuery>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let site = sqlx::query_as::<_, SiteRepository>(
        "SELECT repository_url, branch FROM sites WHERE id = ?",
    )
    .bind(id.as_str())
    .fetch_optional(&state.db)
    .await
    .context("failed to load site repository")?
    .ok_or_else(|| ApiError::NotFound("site not found".into()))?;
    let branch = query.branch.as_deref().unwrap_or(&site.branch);
    if let Err(error) = state.git.list_tree(&id, branch, &query.path).await {
        if error.to_string().contains("cache does not exist") {
            state
                .git
                .fetch(&id, &site.repository_url)
                .await
                .map_err(|error| {
                    ApiError::BadRequest(format!("repository fetch failed: {error}"))
                })?;
        } else {
            return Err(ApiError::BadRequest(format!(
                "could not browse repository: {error}"
            )));
        }
    }
    let entries = state
        .git
        .list_tree(&id, branch, &query.path)
        .await
        .map_err(|error| ApiError::BadRequest(format!("could not browse repository: {error}")))?;
    Ok(HttpResponse::Ok().json(entries))
}

pub async fn get_deploy_key(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    ensure_site(&state, &id).await?;
    let public_key = state
        .git
        .deploy_key(&id)
        .await
        .context("failed to load deploy key")?;
    Ok(HttpResponse::Ok().json(DeployKey { public_key }))
}

pub async fn generate_deploy_key(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    ensure_site(&state, &id).await?;
    let public_key =
        state.git.generate_deploy_key(&id).await.map_err(|error| {
            ApiError::Conflict(format!("could not generate deploy key: {error}"))
        })?;
    tracing::info!(site_id = %id, "SSH deploy key generated");
    Ok(HttpResponse::Created().json(DeployKey {
        public_key: Some(public_key),
    }))
}

pub async fn delete_deploy_key(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    ensure_site(&state, &id).await?;
    state
        .git
        .delete_deploy_key(&id)
        .await
        .context("failed to delete deploy key")?;
    tracing::info!(site_id = %id, "SSH deploy key deleted");
    Ok(HttpResponse::NoContent().finish())
}

pub fn routes(config: &mut web::ServiceConfig) {
    config
        .route("/repositories/inspect", web::post().to(inspect))
        .route("/sites/{id}/repository/refresh", web::post().to(refresh))
        .route("/sites/{id}/repository/tree", web::get().to(tree))
        .route(
            "/sites/{id}/repository/deploy-key",
            web::get().to(get_deploy_key),
        )
        .route(
            "/sites/{id}/repository/deploy-key",
            web::post().to(generate_deploy_key),
        )
        .route(
            "/sites/{id}/repository/deploy-key",
            web::delete().to(delete_deploy_key),
        );
}
