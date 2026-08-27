use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    process::Stdio,
};

use actix_web::{HttpRequest, HttpResponse, http::header, web};
use anyhow::{Context, Result, bail};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tokio::process::Command;
use uuid::Uuid;

use crate::{
    auth::require_session,
    detection::{BuildSuggestions, detect_within},
    error::ApiError,
    state::AppState,
};

#[derive(Clone, Copy)]
enum Status {
    Fetching,
    CheckingOut,
    Preparing,
    InstallingTools,
    InstallingDependencies,
    Building,
    Publishing,
    Validating,
    Activating,
    Success,
}
impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fetching => "fetching",
            Self::CheckingOut => "checking_out",
            Self::Preparing => "preparing",
            Self::InstallingTools => "installing_tools",
            Self::InstallingDependencies => "installing_dependencies",
            Self::Building => "building",
            Self::Publishing => "publishing",
            Self::Validating => "validating",
            Self::Activating => "activating",
            Self::Success => "success",
        }
    }
}

#[derive(Clone, FromRow, Serialize)]
pub struct Deployment {
    pub id: String,
    site_id: String,
    commit_sha: Option<String>,
    commit_message: Option<String>,
    commit_author: Option<String>,
    status: String,
    triggered_by: String,
    build_settings_snapshot: String,
    config_snapshot: Option<String>,
    release_path: Option<String>,
    error_summary: Option<String>,
    log: String,
    created_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    rollback_of_deployment_id: Option<String>,
}

#[derive(FromRow)]
struct SiteBuild {
    id: String,
    repository_url: String,
    branch: String,
    project_directory: String,
    mise_tools: String,
    install_command: Option<String>,
    build_command: Option<String>,
    publish_directory: String,
    build_enabled: bool,
}

pub async fn create(
    req: HttpRequest,
    state: web::Data<AppState>,
    site_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    Ok(HttpResponse::Accepted().json(enqueue(&state, site_id.as_str()).await?))
}

pub async fn enqueue(state: &web::Data<AppState>, site_id: &str) -> Result<Deployment, ApiError> {
    let site = load_site(state, site_id).await?;
    let id = Uuid::new_v4().to_string();
    let snapshot = serde_json::to_string(&serde_json::json!({"project_directory":site.project_directory,"mise_tools":site.mise_tools,"install_command":site.install_command,"build_command":site.build_command,"publish_directory":site.publish_directory,"build_enabled":site.build_enabled})).unwrap();
    let config_snapshot: Option<String> =
        sqlx::query_scalar("SELECT config_json FROM site_chimney_configs WHERE site_id = ?")
            .bind(site_id)
            .fetch_optional(&state.db)
            .await
            .context("failed to snapshot configuration")?;
    let result = sqlx::query("INSERT INTO deployments (id, site_id, status, build_settings_snapshot, config_snapshot) VALUES (?, ?, 'queued', ?, ?)")
        .bind(&id).bind(site_id).bind(snapshot).bind(config_snapshot).execute(&state.db).await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|error| error.is_unique_violation())
        {
            return Err(ApiError::Conflict(
                "this site already has a queued or active deployment".into(),
            ));
        }
        return Err(ApiError::Internal(error.into()));
    }
    let worker_state = state.get_ref().clone();
    let worker_id = id.clone();
    tokio::spawn(async move {
        let permit = worker_state.build_slots.clone().acquire_owned().await;
        if permit.is_err() {
            fail(&worker_state, &worker_id, "deployment queue stopped").await;
            return;
        }
        if let Err(error) = run(&worker_state, &worker_id, &site).await {
            fail(&worker_state, &worker_id, &error.to_string()).await;
        }
    });
    tracing::info!(deployment_id = %id, site_id = %site_id, "deployment queued");
    get_deployment(state, &id).await
}

async fn run(state: &AppState, id: &str, site: &SiteBuild) -> Result<()> {
    transition(state, id, Status::Fetching, "Fetching repository").await?;
    let github_token = crate::github::token_for_repository(state, &site.repository_url).await?;
    state
        .git
        .fetch_with_token(&site.id, &site.repository_url, github_token.as_deref())
        .await?;
    let commit = state.git.resolve_commit(&site.id, &site.branch).await?;
    sqlx::query(
        "UPDATE deployments SET commit_sha=?, commit_message=?, commit_author=? WHERE id=?",
    )
    .bind(&commit.sha)
    .bind(&commit.message)
    .bind(format!("{} <{}>", commit.author_name, commit.author_email))
    .bind(id)
    .execute(&state.db)
    .await?;
    transition(state, id, Status::CheckingOut, "Creating isolated worktree").await?;
    let worktree = state.git.create_worktree(&site.id, id, &commit.sha).await?;
    let result = build_and_activate(state, id, site, worktree.path()).await;
    if let Err(error) = worktree.remove().await {
        append_log(
            state,
            id,
            &format!("\n[warning] failed to clean worktree: {error}\n"),
        )
        .await?;
    }
    result?;
    transition(state, id, Status::Success, "Deployment completed").await?;
    sqlx::query("UPDATE deployments SET finished_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(id)
        .execute(&state.db)
        .await?;
    cleanup_releases(state, &site.id).await?;
    tracing::info!(deployment_id=id,site_id=%site.id,"deployment completed");
    Ok(())
}

async fn build_and_activate(
    state: &AppState,
    id: &str,
    site: &SiteBuild,
    worktree: &Path,
) -> Result<()> {
    transition(state, id, Status::Preparing, "Validating project directory").await?;
    let root = tokio::fs::canonicalize(worktree).await?;
    let project = tokio::fs::canonicalize(worktree.join(&site.project_directory))
        .await
        .context("project directory does not exist")?;
    if !project.starts_with(&root) {
        bail!("project directory escapes repository worktree")
    }
    let mut suggestions = detect_within(&project, &root).await?;
    if let Some(framework) = suggestions.detected_framework.as_deref() {
        sqlx::query("UPDATE sites SET detected_framework = ? WHERE id = ?")
            .bind(framework)
            .bind(&site.id)
            .execute(&state.db)
            .await?;
    }
    let configured_tools: Vec<String> = site
        .mise_tools
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if !configured_tools.is_empty() {
        suggestions.tools = configured_tools;
    }
    if site.build_enabled {
        transition(
            state,
            id,
            Status::InstallingTools,
            "Installing dependencies",
        )
        .await?;
        if project.join("mise.toml").exists() {
            run_process(
                state,
                id,
                &project,
                "mise",
                &["trust", "--yes", "mise.toml"],
            )
            .await?
        }
        if project.join(".mise.toml").exists() {
            run_process(
                state,
                id,
                &project,
                "mise",
                &["trust", "--yes", ".mise.toml"],
            )
            .await?
        }
        let mut install = vec!["install", "--yes"];
        for tool in &suggestions.tools {
            install.push(tool)
        }
        run_process(state, id, &project, "mise", &install).await?;
        if let Some(command) = &site.install_command {
            transition(
                state,
                id,
                Status::InstallingDependencies,
                "Installing dependencies",
            )
            .await?;
            run_mise_command(state, id, &project, &suggestions, command).await?;
        }
        if let Some(command) = &site.build_command {
            transition(state, id, Status::Building, "Building project").await?;
            run_mise_command(state, id, &project, &suggestions, command).await?;
        }
    }
    transition(
        state,
        id,
        Status::Validating,
        "Validating publish directory",
    )
    .await?;
    let publish = tokio::fs::canonicalize(project.join(&site.publish_directory))
        .await
        .context("publish directory does not exist")?;
    if !publish.starts_with(&root) {
        bail!("publish directory escapes build workspace")
    }
    transition(state, id, Status::Publishing, "Creating immutable release").await?;
    let releases = state
        .config
        .data_dir
        .join("sites")
        .join(&site.id)
        .join("releases");
    tokio::fs::create_dir_all(&releases).await?;
    let release = releases.join(id);
    let staging = releases.join(format!(".{id}.tmp"));
    let source = publish.clone();
    let target = staging.clone();
    if let Err(error) = tokio::task::spawn_blocking(move || copy_tree(&source, &target)).await? {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(error);
    }
    tokio::fs::rename(&staging, &release).await?;
    transition(state, id, Status::Activating, "Activating release").await?;
    activate_and_reload(state, &site.id, id).await?;
    sqlx::query("UPDATE deployments SET release_path=? WHERE id=?")
        .bind(release.to_string_lossy().as_ref())
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

async fn run_mise_command(
    state: &AppState,
    id: &str,
    project: &Path,
    suggestions: &BuildSuggestions,
    command: &str,
) -> Result<()> {
    let command = if suggestions.isolate_package_workspace {
        isolate_pnpm_install(command)
    } else {
        Cow::Borrowed(command)
    };
    let mut args = vec!["exec"];
    for tool in &suggestions.tools {
        args.push(tool)
    }
    args.extend(["--command", command.as_ref()]);
    run_process_inner(
        state,
        id,
        project,
        "mise",
        &args,
        suggestions.isolate_package_workspace,
    )
    .await
}

async fn run_process(
    state: &AppState,
    id: &str,
    directory: &Path,
    program: &str,
    args: &[&str],
) -> Result<()> {
    run_process_inner(state, id, directory, program, args, false).await
}

async fn run_process_inner(
    state: &AppState,
    id: &str,
    directory: &Path,
    program: &str,
    args: &[&str],
    isolate_package_workspace: bool,
) -> Result<()> {
    let build_home = state.config.data_dir.join("state/build-home");
    tokio::fs::create_dir_all(&build_home).await?;
    append_log(state, id, &format!("$ {program} {}\n", args.join(" "))).await?;
    let executable = if program == "mise" {
        state
            .config
            .mise_bin
            .as_deref()
            .context("dependency installer is unavailable on the Blank server")?
    } else {
        Path::new(program)
    };
    let executable_path = executable
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
    let child_path = std::env::join_paths([
        executable_path,
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ])?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", child_path)
        .env("HOME", &build_home)
        .env("MISE_DATA_DIR", state.config.data_dir.join("state/mise"))
        .env(
            "MISE_CONFIG_DIR",
            state.config.data_dir.join("state/mise-config"),
        )
        .env("CI", "true")
        .env("MISE_YES", "1");
    if let Some(parent) = directory.parent() {
        command.env("MISE_CEILING_PATHS", parent);
    }
    if isolate_package_workspace {
        command.env("NPM_CONFIG_IGNORE_WORKSPACE", "true");
    }
    let output = command
        .output()
        .await
        .with_context(|| format!("failed to start {program} at {}", executable.display()))?;
    append_log(state, id, &String::from_utf8_lossy(&output.stdout)).await?;
    append_log(state, id, &String::from_utf8_lossy(&output.stderr)).await?;
    if !output.status.success() {
        bail!("command exited with {}", output.status)
    }
    Ok(())
}

fn isolate_pnpm_install(command: &str) -> Cow<'_, str> {
    let trimmed = command.trim();
    if let Some(arguments) = trimmed.strip_prefix("pnpm install") {
        Cow::Owned(format!("pnpm --ignore-workspace install{arguments}"))
    } else {
        Cow::Borrowed(command)
    }
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        bail!("release already exists")
    }
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = target.join(entry.file_name());
        if ty.is_symlink() {
            bail!(
                "publish directory contains a symlink: {}",
                entry.path().display()
            )
        } else if ty.is_dir() {
            copy_tree(&entry.path(), &to)?
        } else if ty.is_file() {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn activate(data_dir: &Path, site_id: &str, deployment_id: &str) -> Result<()> {
    use std::os::unix::fs::symlink;
    let site = data_dir.join("sites").join(site_id);
    let current = site.join("current");
    let temporary = site.join(format!(".current-{deployment_id}"));
    symlink(PathBuf::from("releases").join(deployment_id), &temporary)?;
    std::fs::rename(temporary, current)?;
    Ok(())
}
#[cfg(not(unix))]
fn activate(_: &Path, _: &str, _: &str) -> Result<()> {
    bail!("atomic activation is currently supported on Unix only")
}

async fn activate_and_reload(state: &AppState, site_id: &str, release_id: &str) -> Result<()> {
    let current = state
        .config
        .data_dir
        .join("sites")
        .join(site_id)
        .join("current");
    let previous = std::fs::read_link(&current).ok();
    activate(&state.config.data_dir, site_id, release_id)?;
    if let Err(error) = state.chimney.reload(&state.db).await {
        if let Some(previous) = previous {
            restore_activation(&state.config.data_dir, site_id, &previous)?;
        } else if let Err(remove_error) = std::fs::remove_file(&current)
            && remove_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(remove_error)
                .context("failed to restore inactive site after reload failure");
        }
        let _ = state.chimney.reload(&state.db).await;
        return Err(error).context("Chimney rejected the activated release");
    }
    Ok(())
}

#[cfg(unix)]
fn restore_activation(data_dir: &Path, site_id: &str, target: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;
    let site = data_dir.join("sites").join(site_id);
    let temporary = site.join(".current-restore");
    if temporary.exists() {
        std::fs::remove_file(&temporary)?;
    }
    symlink(target, &temporary)?;
    std::fs::rename(temporary, site.join("current"))?;
    Ok(())
}

#[cfg(not(unix))]
fn restore_activation(_: &Path, _: &str, _: &Path) -> Result<()> {
    bail!("atomic activation is currently supported on Unix only")
}

async fn cleanup_releases(state: &AppState, site_id: &str) -> Result<()> {
    let releases = state
        .config
        .data_dir
        .join("sites")
        .join(site_id)
        .join("releases");
    let retained: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM deployments WHERE site_id=? AND status='success' AND release_path IS NOT NULL AND rollback_of_deployment_id IS NULL ORDER BY finished_at DESC LIMIT ?",
    )
    .bind(site_id)
    .bind(state.config.release_retention as i64)
    .fetch_all(&state.db)
    .await?;
    let active = std::fs::read_link(
        state
            .config
            .data_dir
            .join("sites")
            .join(site_id)
            .join("current"),
    )
    .ok()
    .and_then(|path| path.file_name().map(|name| name.to_owned()));
    let mut entries = match tokio::fs::read_dir(&releases).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name_string = name.to_string_lossy();
        if active.as_ref() == Some(&name) || retained.iter().any(|id| id == &name_string) {
            continue;
        }
        if entry.file_type().await?.is_dir() {
            tokio::fs::remove_dir_all(entry.path()).await?;
            tracing::info!(site_id, release=%name_string, "removed expired release");
        }
    }
    Ok(())
}

async fn transition(state: &AppState, id: &str, status: Status, message: &str) -> Result<()> {
    sqlx::query("UPDATE deployments SET status=?, started_at=COALESCE(started_at,CURRENT_TIMESTAMP), log=log||?||char(10) WHERE id=?").bind(status.as_str()).bind(format!("[{}] {message}",status.as_str())).bind(id).execute(&state.db).await?;
    Ok(())
}
async fn append_log(state: &AppState, id: &str, text: &str) -> Result<()> {
    sqlx::query("UPDATE deployments SET log=log||? WHERE id=?")
        .bind(text)
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}
async fn fail(state: &AppState, id: &str, message: &str) {
    let _=sqlx::query("UPDATE deployments SET status='failed',error_summary=?,finished_at=CURRENT_TIMESTAMP,log=log||? WHERE id=?").bind(message).bind(format!("\n[failed] {message}\n")).bind(id).execute(&state.db).await;
    tracing::error!(deployment_id = id, error = message, "deployment failed");
}
async fn load_site(state: &AppState, id: &str) -> Result<SiteBuild, ApiError> {
    sqlx::query_as("SELECT id,repository_url,branch,project_directory,mise_tools,install_command,build_command,publish_directory,build_enabled FROM sites WHERE id=?").bind(id).fetch_optional(&state.db).await.context("failed to load site")?.ok_or_else(||ApiError::NotFound("site not found".into()))
}
async fn get_deployment(state: &AppState, id: &str) -> Result<Deployment, ApiError> {
    sqlx::query_as("SELECT id,site_id,commit_sha,commit_message,commit_author,status,triggered_by,build_settings_snapshot,config_snapshot,release_path,error_summary,log,created_at,started_at,finished_at,rollback_of_deployment_id FROM deployments WHERE id=?").bind(id).fetch_optional(&state.db).await.context("failed to load deployment")?.ok_or_else(||ApiError::NotFound("deployment not found".into()))
}
pub async fn get(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    Ok(HttpResponse::Ok().json(get_deployment(&state, &id).await?))
}

pub async fn rollback(
    req: HttpRequest,
    state: web::Data<AppState>,
    target_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let target = get_deployment(&state, &target_id).await?;
    if target.status != "success" {
        return Err(ApiError::Conflict(
            "only successful deployments can be rolled back".into(),
        ));
    }
    let release_path = target
        .release_path
        .as_deref()
        .ok_or_else(|| ApiError::Conflict("deployment has no retained release".into()))?;
    let release = tokio::fs::canonicalize(release_path).await.map_err(|_| {
        ApiError::Conflict("this deployment's release is no longer retained".into())
    })?;
    let release_root = tokio::fs::canonicalize(
        state
            .config
            .data_dir
            .join("sites")
            .join(&target.site_id)
            .join("releases"),
    )
    .await
    .context("failed to resolve release directory")?;
    if release.parent() != Some(release_root.as_path()) {
        return Err(ApiError::Conflict(
            "deployment release path is invalid".into(),
        ));
    }
    let release_id = release
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ApiError::Conflict("deployment release path is invalid".into()))?;
    let id = Uuid::new_v4().to_string();
    let inserted = sqlx::query("INSERT INTO deployments (id,site_id,commit_sha,commit_message,commit_author,status,triggered_by,build_settings_snapshot,config_snapshot,release_path,log,started_at,rollback_of_deployment_id) VALUES (?,?,?,?,?,'activating','rollback',?,?,?,?,CURRENT_TIMESTAMP,?)")
        .bind(&id).bind(&target.site_id).bind(&target.commit_sha).bind(&target.commit_message).bind(&target.commit_author).bind(&target.build_settings_snapshot).bind(&target.config_snapshot).bind(release.to_string_lossy().as_ref()).bind(format!("[activating] Rolling back to {}\n", target.id)).bind(&target.id).execute(&state.db).await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .is_some_and(|error| error.is_unique_violation())
        {
            return Err(ApiError::Conflict(
                "this site already has an active deployment".into(),
            ));
        }
        return Err(ApiError::Internal(error.into()));
    }
    if let Err(error) = activate_and_reload(&state, &target.site_id, release_id).await {
        fail(&state, &id, &error.to_string()).await;
        return Err(ApiError::Internal(error));
    }
    sqlx::query("UPDATE deployments SET status='success',finished_at=CURRENT_TIMESTAMP,log=log||'[success] Rollback completed'||char(10) WHERE id=?")
        .bind(&id)
        .execute(&state.db)
        .await
        .context("rollback activated but history update failed")?;
    tracing::info!(deployment_id=%id, target_deployment_id=%target.id, site_id=%target.site_id, "rollback completed");
    Ok(HttpResponse::Created().json(get_deployment(&state, &id).await?))
}

#[derive(FromRow)]
struct LogCursor {
    chunk: String,
    offset: i64,
    status: String,
}

pub async fn events(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    get_deployment(&state, &id).await?;
    let offset = req
        .headers()
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let stream_state = (
        state.get_ref().clone(),
        id.into_inner(),
        offset,
        String::new(),
        0_u8,
    );
    let body = stream::unfold(
        stream_state,
        |(state, id, offset, previous_status, idle)| async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let cursor = sqlx::query_as::<_, LogCursor>(
            "SELECT substr(log, ? + 1) AS chunk, length(log) AS offset, status FROM deployments WHERE id=?",
        )
        .bind(offset)
        .bind(&id)
        .fetch_optional(&state.db)
        .await;
            let cursor = match cursor {
                Ok(Some(cursor)) => cursor,
                Ok(None) => return None,
                Err(error) => {
                    tracing::warn!(deployment_id=%id, ?error, "deployment event stream stopped");
                    return None;
                }
            };
            let finished = matches!(cursor.status.as_str(), "success" | "failed" | "cancelled");
            let changed = !cursor.chunk.is_empty() || cursor.status != previous_status;
            let next_idle = if changed { 0 } else { idle.saturating_add(1) };
            let output = if changed {
                let data = serde_json::json!({"chunk": cursor.chunk, "status": cursor.status, "done": finished});
                Some(format!(
                    "id: {}\nevent: deployment\ndata: {}\n\n",
                    cursor.offset, data
                ))
            } else if next_idle >= 40 {
                Some(": keep-alive\n\n".into())
            } else {
                None
            };
            let next = (
                state,
                id,
                cursor.offset,
                cursor.status,
                if next_idle >= 40 { 0 } else { next_idle },
            );
            if finished && output.is_none() {
                return None;
            }
            Some((
                Ok::<_, actix_web::Error>(web::Bytes::from(output.unwrap_or_default())),
                next,
            ))
        },
    );
    Ok(HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((header::CACHE_CONTROL, "no-cache, no-transform"))
        .insert_header((header::CONNECTION, "keep-alive"))
        .insert_header((header::CONTENT_ENCODING, "identity"))
        .streaming(body))
}
pub async fn list(
    req: HttpRequest,
    state: web::Data<AppState>,
    site_id: web::Path<String>,
    query: web::Query<DeploymentListQuery>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let pattern = format!("%{}%", query.search.trim());
    let status = query.status.trim();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deployments WHERE site_id=? AND (?='' OR commit_message LIKE ? OR commit_sha LIKE ? OR status LIKE ? OR triggered_by LIKE ?) AND (?='' OR status=?)")
        .bind(site_id.as_str()).bind(query.search.trim()).bind(&pattern).bind(&pattern).bind(&pattern).bind(&pattern).bind(status).bind(status).fetch_one(&state.db).await.context("failed to count deployments")?;
    let limit = query.limit.clamp(1, 100);
    let offset = query.offset.max(0);
    let rows=sqlx::query_as::<_,Deployment>("SELECT id,site_id,commit_sha,commit_message,commit_author,status,triggered_by,build_settings_snapshot,config_snapshot,release_path,error_summary,'' AS log,created_at,started_at,finished_at,rollback_of_deployment_id FROM deployments WHERE site_id=? AND (?='' OR commit_message LIKE ? OR commit_sha LIKE ? OR status LIKE ? OR triggered_by LIKE ?) AND (?='' OR status=?) ORDER BY created_at DESC LIMIT ? OFFSET ?")
        .bind(site_id.as_str()).bind(query.search.trim()).bind(&pattern).bind(&pattern).bind(&pattern).bind(&pattern).bind(status).bind(status).bind(limit).bind(offset).fetch_all(&state.db).await.context("failed to list deployments")?;
    Ok(HttpResponse::Ok().json(DeploymentPage {
        items: rows,
        total,
        offset,
        limit,
    }))
}

#[derive(serde::Deserialize)]
pub struct DeploymentListQuery {
    #[serde(default)]
    search: String,
    #[serde(default)]
    status: String,
    #[serde(default = "default_deployment_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn default_deployment_limit() -> i64 {
    25
}

#[derive(serde::Serialize)]
struct DeploymentPage {
    items: Vec<Deployment>,
    total: i64,
    offset: i64,
    limit: i64,
}
pub async fn suggestions(
    req: HttpRequest,
    state: web::Data<AppState>,
    site_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let site = load_site(&state, &site_id).await?;
    let token = crate::github::token_for_repository(&state, &site.repository_url).await?;
    state
        .git
        .fetch_with_token(&site.id, &site.repository_url, token.as_deref())
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let commit = state
        .git
        .resolve_commit(&site.id, &site.branch)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let wid = format!("detect-{}", Uuid::new_v4());
    let wt = state
        .git
        .create_worktree(&site.id, &wid, &commit.sha)
        .await
        .context("failed to checkout repository")?;
    let root = tokio::fs::canonicalize(wt.path())
        .await
        .context("failed to resolve detection worktree")?;
    let project = tokio::fs::canonicalize(wt.path().join(&site.project_directory))
        .await
        .context("project directory does not exist")?;
    let result = if project.starts_with(&root) {
        detect_within(&project, &root).await
    } else {
        Err(anyhow::anyhow!(
            "project directory escapes repository worktree"
        ))
    };
    wt.remove()
        .await
        .context("failed to clean detection worktree")?;
    Ok(HttpResponse::Ok().json(result?))
}

#[derive(Deserialize)]
pub struct DraftSuggestionsInput {
    repository_url: String,
    branch: String,
    #[serde(default = "default_project_directory")]
    project_directory: String,
}

fn default_project_directory() -> String {
    ".".into()
}

pub async fn draft_suggestions(
    req: HttpRequest,
    state: web::Data<AppState>,
    input: web::Json<DraftSuggestionsInput>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    let cache_id = format!("draft-{}", Uuid::new_v4());
    let result = async {
        let token =
            crate::github::token_for_repository(&state, input.repository_url.trim()).await?;
        state
            .git
            .fetch_with_token(&cache_id, input.repository_url.trim(), token.as_deref())
            .await
            .map_err(|error| ApiError::BadRequest(format!("repository fetch failed: {error}")))?;
        let commit = state
            .git
            .resolve_commit(&cache_id, input.branch.trim())
            .await
            .map_err(|error| ApiError::BadRequest(format!("branch resolution failed: {error}")))?;
        let worktree_id = format!("detect-{}", Uuid::new_v4());
        let worktree = state
            .git
            .create_worktree(&cache_id, &worktree_id, &commit.sha)
            .await
            .map_err(|error| {
                ApiError::BadRequest(format!("repository checkout failed: {error}"))
            })?;
        let root = tokio::fs::canonicalize(worktree.path())
            .await
            .context("failed to resolve detection worktree")?;
        let project = tokio::fs::canonicalize(worktree.path().join(input.project_directory.trim()))
            .await
            .map_err(|_| ApiError::BadRequest("project directory does not exist".into()))?;
        if !project.starts_with(&root) {
            return Err(ApiError::BadRequest(
                "project directory escapes repository worktree".into(),
            ));
        }
        let suggestions = detect_within(&project, &root)
            .await
            .map_err(|error| ApiError::BadRequest(format!("build detection failed: {error}")))?;
        worktree
            .remove()
            .await
            .context("failed to clean detection worktree")?;
        Ok(HttpResponse::Ok().json(suggestions))
    }
    .await;
    if let Err(error) = state.git.delete_site_data(&cache_id).await {
        tracing::warn!(%error, "failed to remove draft repository cache");
    }
    result
}
pub fn routes(config: &mut web::ServiceConfig) {
    config
        .route("/repositories/detect", web::post().to(draft_suggestions))
        .route("/sites/{id}/deployments", web::get().to(list))
        .route("/sites/{id}/deployments", web::post().to(create))
        .route("/sites/{id}/repository/detect", web::post().to(suggestions))
        .route("/deployments/{id}", web::get().to(get))
        .route("/deployments/{id}/rollback", web::post().to(rollback))
        .route("/deployments/{id}/events", web::get().to(events));
}

pub async fn recover_interrupted(db: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query("UPDATE deployments SET status='failed', error_summary='Blank restarted during deployment', finished_at=CURRENT_TIMESTAMP, log=log||char(10)||'[failed] Blank restarted during deployment'||char(10) WHERE status IN ('queued','fetching','checking_out','preparing','installing_tools','installing_dependencies','building','publishing','validating','activating')")
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_rejects_symlinks() {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("s");
        std::fs::create_dir(&s).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/passwd", s.join("bad")).unwrap();
            assert!(copy_tree(&s, &t.path().join("out")).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn activation_switches_current_release() {
        let t = tempfile::tempdir().unwrap();
        let releases = t.path().join("sites/site/releases");
        std::fs::create_dir_all(releases.join("old")).unwrap();
        std::fs::create_dir(releases.join("new")).unwrap();
        std::os::unix::fs::symlink("releases/old", t.path().join("sites/site/current")).unwrap();
        activate(t.path(), "site", "new").unwrap();
        assert_eq!(
            std::fs::read_link(t.path().join("sites/site/current")).unwrap(),
            PathBuf::from("releases/new")
        );
    }

    #[test]
    fn isolates_default_pnpm_install_from_parent_workspaces() {
        assert_eq!(
            isolate_pnpm_install("pnpm install --frozen-lockfile"),
            "pnpm --ignore-workspace install --frozen-lockfile"
        );
        assert_eq!(isolate_pnpm_install("pnpm run build"), "pnpm run build");
    }
}
