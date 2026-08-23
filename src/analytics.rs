use actix_web::{HttpRequest, HttpResponse, web};
use chimney::server::{RequestEvent, RequestProtocol};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::{auth::require_session, error::ApiError, state::AppState};

#[derive(Clone)]
pub struct Recorder {
    sender: mpsc::UnboundedSender<RequestRecord>,
}

struct RequestRecord {
    site_id: String,
    host: Option<String>,
    method: String,
    path: String,
    status: u16,
    duration_ms: i64,
    protocol: &'static str,
}

impl Recorder {
    pub fn start(db: SqlitePool) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<RequestRecord>();
        tokio::spawn(async move {
            while let Some(record) = receiver.recv().await {
                if let Err(error) = sqlx::query("INSERT INTO site_request_logs (site_id, host, method, path, status, duration_ms, protocol) VALUES (?, ?, ?, ?, ?, ?, ?)")
                    .bind(record.site_id)
                    .bind(record.host)
                    .bind(record.method)
                    .bind(record.path)
                    .bind(record.status as i64)
                    .bind(record.duration_ms)
                    .bind(record.protocol)
                    .execute(&db)
                    .await
                {
                    tracing::warn!(?error, "failed to record site request analytics");
                }
            }
        });
        Self { sender }
    }

    pub fn record(&self, event: &RequestEvent) {
        let Some(site_id) = event.site.clone() else {
            return;
        };
        let path = event
            .uri
            .path_and_query()
            .map(|value| value.as_str())
            .unwrap_or(event.uri.path())
            .chars()
            .take(2048)
            .collect();
        let protocol = match event.protocol {
            RequestProtocol::Http => "http",
            RequestProtocol::Https => "https",
        };
        let _ = self.sender.send(RequestRecord {
            site_id,
            host: event.host.clone(),
            method: event.method.as_str().to_owned(),
            path,
            status: event.status.as_u16(),
            duration_ms: event.duration.as_millis().min(i64::MAX as u128) as i64,
            protocol,
        });
    }
}

#[derive(Serialize)]
struct AnalyticsResponse {
    total_requests: i64,
    error_requests: i64,
    average_duration_ms: f64,
    daily: Vec<DailyRow>,
}

#[derive(Serialize, sqlx::FromRow)]
struct DailyRow {
    day: String,
    requests: i64,
    errors: i64,
    average_duration_ms: f64,
}

async fn site_analytics(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<DaysQuery>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, false).await?;
    let site_id = path.into_inner();
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sites WHERE id = ?")
        .bind(&site_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| ApiError::NotFound("site not found".into()))?;
    if exists == 0 {
        return Err(ApiError::NotFound("site not found".into()));
    }
    let days = query.days.clamp(1, 365);
    let total_requests = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM site_request_logs WHERE site_id = ? AND created_at >= datetime('now', ?)")
        .bind(&site_id).bind(format!("-{days} days")).fetch_one(&state.db).await.map_err(anyhow::Error::from)?;
    let error_requests = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM site_request_logs WHERE site_id = ? AND status >= 400 AND created_at >= datetime('now', ?)")
        .bind(&site_id).bind(format!("-{days} days")).fetch_one(&state.db).await.map_err(anyhow::Error::from)?;
    let average_duration_ms = sqlx::query_scalar::<_, Option<f64>>("SELECT AVG(duration_ms) FROM site_request_logs WHERE site_id = ? AND created_at >= datetime('now', ?)")
        .bind(&site_id).bind(format!("-{days} days")).fetch_one(&state.db).await.map_err(anyhow::Error::from)?.unwrap_or(0.0);
    let daily = sqlx::query_as::<_, DailyRow>("SELECT date(created_at) AS day, COUNT(*) AS requests, SUM(CASE WHEN status >= 400 THEN 1 ELSE 0 END) AS errors, AVG(duration_ms) AS average_duration_ms FROM site_request_logs WHERE site_id = ? AND created_at >= datetime('now', ?) GROUP BY date(created_at) ORDER BY day DESC")
        .bind(&site_id).bind(format!("-{days} days")).fetch_all(&state.db).await.map_err(anyhow::Error::from)?;
    Ok(HttpResponse::Ok().json(AnalyticsResponse {
        total_requests,
        error_requests,
        average_duration_ms,
        daily,
    }))
}

#[derive(serde::Deserialize)]
struct DaysQuery {
    #[serde(default = "default_days")]
    days: i64,
}
fn default_days() -> i64 {
    30
}

pub fn routes(config: &mut web::ServiceConfig) {
    config.route("/sites/{id}/analytics", web::get().to(site_analytics));
}
