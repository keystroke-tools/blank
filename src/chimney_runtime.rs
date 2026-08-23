use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use chimney::{
    config::{Config as ChimneyConfig, ConfigHandle, HttpsConfig, Site, Sites},
    filesystem::local::LocalFS,
    server::Server,
};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use tokio::sync::RwLock;

use crate::config::Config;

#[derive(Clone)]
pub struct ChimneyRuntime {
    handle: ConfigHandle,
    settings: Config,
    status: Arc<RwLock<RuntimeStatus>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeState {
    Starting,
    Running,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub active_sites: usize,
    pub http_address: String,
    pub https_port: Option<u16>,
    pub error: Option<String>,
}

#[derive(FromRow)]
struct StoredSite {
    id: String,
    config_json: String,
}

const MISSING_DEPLOYMENT_PAGE: &str = include_str!("../assets/missing-deployment.html");

impl ChimneyRuntime {
    pub async fn start(db: &SqlitePool, settings: &Config) -> Result<Self> {
        let config = build_config(db, settings).await?;
        let active_sites = config.sites.len();
        let handle = ConfigHandle::from(config);
        let tls_at_start = settings.chimney_https_port.is_some() && active_sites > 0;
        let status = Arc::new(RwLock::new(RuntimeStatus {
            state: RuntimeState::Starting,
            active_sites,
            http_address: settings.chimney_bind.to_string(),
            https_port: tls_at_start
                .then_some(settings.chimney_https_port)
                .flatten(),
            error: None,
        }));
        let filesystem = Arc::new(
            LocalFS::new(settings.data_dir.join("sites"))
                .context("failed to initialize Chimney filesystem")?,
        );
        let server = if tls_at_start {
            Server::new_with_tls(filesystem, handle.clone())
                .await
                .context("failed to initialize Chimney TLS")?
        } else {
            Server::new(filesystem, handle.clone())
        };
        let task_status = status.clone();
        tokio::spawn(async move {
            task_status.write().await.state = RuntimeState::Running;
            if let Err(error) = server.run().await {
                let mut status = task_status.write().await;
                status.state = RuntimeState::Failed;
                status.error = Some(error.to_string());
                tracing::error!(?error, "embedded Chimney stopped");
            }
        });
        Ok(Self {
            handle,
            settings: settings.clone(),
            status,
        })
    }

    pub async fn reload(&self, db: &SqlitePool) -> Result<()> {
        let config = build_config(db, &self.settings).await?;
        let active_sites = config.sites.len();
        self.handle
            .set(config)
            .context("failed to update embedded Chimney configuration")?;
        self.status.write().await.active_sites = active_sites;
        Ok(())
    }

    pub async fn status(&self) -> RuntimeStatus {
        self.status.read().await.clone()
    }
}

async fn build_config(db: &SqlitePool, settings: &Config) -> Result<ChimneyConfig> {
    let mut config = ChimneyConfig::default();
    config.host = settings.chimney_bind.ip();
    config.port = settings.chimney_bind.port();
    config.sites_directory = settings
        .data_dir
        .join("sites")
        .to_string_lossy()
        .into_owned();
    config.https = settings.chimney_https_port.map(|port| HttpsConfig {
        enabled: true,
        port,
        cache_directory: settings.data_dir.join("state/certificates"),
        acme_email: settings.chimney_acme_email.clone(),
        ..HttpsConfig::default()
    });
    let rows = sqlx::query_as::<_, StoredSite>("SELECT s.id, c.config_json FROM sites s JOIN site_chimney_configs c ON c.site_id = s.id ORDER BY s.id")
        .fetch_all(db).await.context("failed to load Chimney sites")?;
    let mut sites = Sites::default();
    for row in rows {
        let current = settings
            .data_dir
            .join("sites")
            .join(&row.id)
            .join("current");
        let mut site: Site =
            serde_json::from_str(&row.config_json).context("stored Chimney site is invalid")?;
        site.domain_names =
            sqlx::query_scalar("SELECT domain FROM site_domains WHERE site_id = ? ORDER BY domain")
                .bind(&row.id)
                .fetch_all(db)
                .await
                .context("failed to load runtime domains")?;
        site.name = row.id;
        if current.exists() {
            validate_current_release(&settings.data_dir, &site.name, &current).await?;
            let configured_root = PathBuf::from(&site.root);
            site.root = PathBuf::from("current")
                .join(&configured_root)
                .to_string_lossy()
                .into_owned();
            if let Some(fallback) = &site.fallback_file {
                site.fallback_file = Some(
                    PathBuf::from(&site.root)
                        .join(fallback)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        } else {
            ensure_missing_deployment_page(&settings.data_dir, &site.name).await?;
            site.root = ".blank/missing-deployment".into();
            site.default_index_file = Some("index.html".into());
            site.fallback_file = Some(".blank/missing-deployment/index.html".into());
        }
        sites.add(site).context("failed to add Chimney site")?;
    }
    config.sites = sites;
    Ok(config)
}

async fn ensure_missing_deployment_page(data_dir: &Path, site_id: &str) -> Result<()> {
    let directory = data_dir
        .join("sites")
        .join(site_id)
        .join(".blank/missing-deployment");
    tokio::fs::create_dir_all(&directory)
        .await
        .context("failed to create missing-deployment page directory")?;
    let path = directory.join("index.html");
    if tokio::fs::read_to_string(&path).await.ok().as_deref() != Some(MISSING_DEPLOYMENT_PAGE) {
        tokio::fs::write(path, MISSING_DEPLOYMENT_PAGE)
            .await
            .context("failed to write missing-deployment page")?;
    }
    Ok(())
}

async fn validate_current_release(data_dir: &Path, site_id: &str, current: &Path) -> Result<()> {
    let site_root = data_dir.join("sites").join(site_id);
    let releases = site_root.join("releases");
    let resolved = tokio::fs::canonicalize(current)
        .await
        .context("active release link is invalid")?;
    let resolved_releases = tokio::fs::canonicalize(&releases)
        .await
        .context("site releases directory is invalid")?;
    if !resolved.starts_with(&resolved_releases) {
        anyhow::bail!("active release escapes its site's releases directory");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[actix_web::test]
    async fn rejects_current_release_outside_release_directory() {
        let temp = tempfile::tempdir().unwrap();
        let site = temp.path().join("sites/site-1");
        tokio::fs::create_dir_all(site.join("releases/good"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(temp.path().join("outside"))
            .await
            .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(temp.path().join("outside"), site.join("current")).unwrap();
        #[cfg(unix)]
        assert!(
            validate_current_release(temp.path(), "site-1", &site.join("current"))
                .await
                .is_err()
        );
    }

    #[actix_web::test]
    async fn writes_the_branded_missing_deployment_page() {
        let temp = tempfile::tempdir().unwrap();
        ensure_missing_deployment_page(temp.path(), "site-1")
            .await
            .unwrap();
        let page = tokio::fs::read_to_string(
            temp.path()
                .join("sites/site-1/.blank/missing-deployment/index.html"),
        )
        .await
        .unwrap();
        assert!(page.contains("404 · Missing deployment"));
        assert!(page.contains("Nothing lives here yet."));
    }
}
