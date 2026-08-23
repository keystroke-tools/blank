use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::chimney_runtime::ChimneyRuntime;
use crate::config::Config;
use crate::git::GitService;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Config,
    pub git: GitService,
    pub chimney: ChimneyRuntime,
    pub build_slots: Arc<Semaphore>,
}
