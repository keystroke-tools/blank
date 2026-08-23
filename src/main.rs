mod analytics;
mod auth;
mod chimney_config;
mod chimney_runtime;
mod config;
mod db;
mod deployment;
mod detection;
mod dns;
mod error;
mod git;
mod repositories;
mod sites;
mod state;
mod web;

use actix_web::{App, HttpResponse, HttpServer, middleware, web as actix_web_data};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing_actix_web::TracingLogger;
use tracing_subscriber::EnvFilter;

use crate::{config::Config, state::AppState};

async fn health(state: actix_web_data::Data<AppState>) -> HttpResponse {
    let database = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();
    let chimney = state.chimney.status().await;
    HttpResponse::Ok().json(serde_json::json!({
        "status": if database { "ok" } else { "degraded" },
        "database": if database { "healthy" } else { "unhealthy" },
        "chimney": chimney,
        "site_http_port": state.config.chimney_bind.port(),
        "site_https_port": state.config.chimney_https_port,
    }))
}

#[actix_web::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "blank=info,actix_web=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    let db = db::connect(&config).await?;
    let analytics = analytics::Recorder::start(db.clone());
    deployment::recover_interrupted(&db).await?;
    let git = git::GitService::new(&config.data_dir);
    git.prepare().await?;
    let chimney = chimney_runtime::ChimneyRuntime::start(&db, &config, analytics).await?;
    let state = actix_web_data::Data::new(AppState {
        db,
        config: config.clone(),
        git,
        chimney,
        build_slots: Arc::new(Semaphore::new(2)),
    });
    let bind = config.bind;
    tracing::info!(%bind, "starting Blank admin server");

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(middleware::Compress::default())
            .wrap(TracingLogger::default())
            .service(
                actix_web_data::scope("/api")
                    .configure(auth::routes)
                    .configure(repositories::routes)
                    .configure(chimney_config::routes)
                    .configure(deployment::routes)
                    .configure(dns::routes)
                    .configure(analytics::routes)
                    .configure(sites::routes)
                    .route("/health", actix_web_data::get().to(health)),
            )
            .route("/{path:.*}", actix_web_data::get().to(web::assets))
    })
    .bind(bind)?
    .run()
    .await?;
    Ok(())
}
