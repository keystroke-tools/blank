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
    ip_address: Option<String>,
    country: Option<String>,
    device_type: &'static str,
    user_agent: Option<String>,
    referer: Option<String>,
}

impl Recorder {
    pub fn start(db: SqlitePool) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<RequestRecord>();
        tokio::spawn(async move {
            while let Some(record) = receiver.recv().await {
                if let Err(error) = sqlx::query("INSERT INTO site_request_logs (site_id, host, method, path, status, duration_ms, protocol, ip_address, country, device_type, user_agent, referer) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(record.site_id)
                    .bind(record.host)
                    .bind(record.method)
                    .bind(record.path)
                    .bind(record.status as i64)
                    .bind(record.duration_ms)
                    .bind(record.protocol)
                    .bind(record.ip_address).bind(record.country).bind(record.device_type)
                    .bind(record.user_agent).bind(record.referer)
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
            ip_address: client_ip(event),
            country: event
                .request_headers
                .get("cf-ipcountry")
                .or_else(|| event.request_headers.get("x-country"))
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
            device_type: device_type(
                event
                    .request_headers
                    .get("user-agent")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default(),
            ),
            user_agent: event
                .request_headers
                .get("user-agent")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.chars().take(512).collect()),
            referer: event
                .request_headers
                .get("referer")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.chars().take(2048).collect()),
        });
    }
}

// Chimney sits behind the local reverse proxy in production, so the socket
// address is the proxy itself. Prefer headers set by that trusted proxy.
fn client_ip(event: &RequestEvent) -> Option<String> {
    for name in ["cf-connecting-ip", "x-real-ip", "x-forwarded-for"] {
        if let Some(value) = event
            .request_headers
            .get(name)
            .and_then(|value| value.to_str().ok())
        {
            let value = value.split(',').next().unwrap_or_default().trim();
            if value.parse::<std::net::IpAddr>().is_ok() {
                return Some(value.to_owned());
            }
        }
    }
    event.remote_addr.map(|address| address.ip().to_string())
}

fn device_type(user_agent: &str) -> &'static str {
    let ua = user_agent.to_ascii_lowercase();
    if ua.contains("mobile") || ua.contains("android") || ua.contains("iphone") {
        "mobile"
    } else if ua.contains("tablet") || ua.contains("ipad") {
        "tablet"
    } else if ua.is_empty() {
        "unknown"
    } else {
        "desktop"
    }
}

#[derive(Serialize)]
struct AnalyticsResponse {
    total_requests: i64,
    error_requests: i64,
    average_duration_ms: f64,
    daily: Vec<DailyRow>,
    requests: Vec<RequestRow>,
    request_total: i64,
    request_offset: i64,
    request_limit: i64,
}

#[derive(Serialize, sqlx::FromRow)]
struct RequestRow {
    id: i64,
    created_at: String,
    host: Option<String>,
    method: String,
    path: String,
    status: i64,
    duration_ms: i64,
    protocol: String,
    ip_address: Option<String>,
    country: Option<String>,
    device_type: String,
    user_agent: Option<String>,
    referer: Option<String>,
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
    let pattern = format!("%{}%", query.search.trim());
    let status = query.status.unwrap_or(0).clamp(0, 599);
    let method = query.method.trim().to_ascii_uppercase();
    let device = query.device.trim();
    let country = query.country.trim();
    let request_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_request_logs WHERE site_id = ? AND created_at >= datetime('now', ?) AND (? = '' OR path LIKE ? OR host LIKE ? OR method LIKE ? OR device_type LIKE ? OR country LIKE ?) AND (? = 0 OR status = ?) AND (? = '' OR method = ?) AND (? = '' OR device_type = ?) AND (? = '' OR country = ?)")
        .bind(&site_id).bind(format!("-{days} days")).bind(query.search.trim()).bind(&pattern).bind(&pattern).bind(&pattern).bind(&pattern).bind(&pattern).bind(status).bind(status).bind(&method).bind(&method).bind(device).bind(device).bind(country).bind(country)
        .fetch_one(&state.db).await.map_err(anyhow::Error::from)?;
    let limit = query.limit.clamp(1, 200);
    let offset = query.offset.max(0);
    let requests = sqlx::query_as::<_, RequestRow>("SELECT id, created_at, host, method, path, status, duration_ms, protocol, ip_address, country, device_type, user_agent, referer FROM site_request_logs WHERE site_id = ? AND created_at >= datetime('now', ?) AND (? = '' OR path LIKE ? OR host LIKE ? OR method LIKE ? OR device_type LIKE ? OR country LIKE ?) AND (? = 0 OR status = ?) AND (? = '' OR method = ?) AND (? = '' OR device_type = ?) AND (? = '' OR country = ?) ORDER BY id DESC LIMIT ? OFFSET ?")
        .bind(&site_id).bind(format!("-{days} days")).bind(query.search.trim()).bind(&pattern).bind(&pattern).bind(&pattern).bind(&pattern).bind(&pattern).bind(status).bind(status).bind(&method).bind(&method).bind(device).bind(device).bind(country).bind(country)
        .bind(limit).bind(offset).fetch_all(&state.db).await.map_err(anyhow::Error::from)?;
    Ok(HttpResponse::Ok().json(AnalyticsResponse {
        total_requests,
        error_requests,
        average_duration_ms,
        daily,
        requests,
        request_total,
        request_offset: offset,
        request_limit: limit,
    }))
}

#[derive(serde::Deserialize)]
struct DaysQuery {
    #[serde(default = "default_days")]
    days: i64,
    #[serde(default)]
    search: String,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    status: Option<i64>,
    #[serde(default)]
    method: String,
    #[serde(default)]
    device: String,
    #[serde(default)]
    country: String,
}
fn default_days() -> i64 {
    30
}
fn default_limit() -> i64 {
    50
}

pub fn routes(config: &mut web::ServiceConfig) {
    config.route("/sites/{id}/analytics", web::get().to(site_analytics));
}
