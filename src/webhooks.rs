use actix_web::{HttpRequest, HttpResponse, web};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{deployment, error::ApiError, state::AppState};

type HmacSha256 = Hmac<Sha256>;

pub async fn github(
    request: HttpRequest,
    state: web::Data<AppState>,
    body: web::Bytes,
) -> Result<HttpResponse, ApiError> {
    if request
        .headers()
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        != Some("push")
    {
        return Ok(HttpResponse::NoContent().finish());
    }
    let secret = state
        .config
        .webhook_secret
        .as_deref()
        .ok_or(ApiError::NotFound("webhooks are not configured".into()))?;
    let signature = request
        .headers()
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let signature = signature
        .strip_prefix("sha256=")
        .ok_or(ApiError::Forbidden)?;
    let expected = hex::decode(signature).map_err(|_| ApiError::Forbidden)?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("invalid webhook secret")))?;
    mac.update(&body);
    mac.verify_slice(&expected)
        .map_err(|_| ApiError::Forbidden)?;
    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::BadRequest("invalid webhook payload".into()))?;
    if payload
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        == ""
    {
        return Err(ApiError::BadRequest("webhook branch is missing".into()));
    }
    let repository = payload
        .pointer("/repository/clone_url")
        .and_then(|v| v.as_str())
        .or_else(|| {
            payload
                .pointer("/repository/html_url")
                .and_then(|v| v.as_str())
        })
        .unwrap_or_default()
        .trim_end_matches(".git");
    let branch = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .and_then(|v| v.strip_prefix("refs/heads/"))
        .unwrap_or_default();
    let site_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM sites WHERE repository_url LIKE ? AND branch = ? AND auto_deploy = 1",
    )
    .bind(format!("{repository}%"))
    .bind(branch)
    .fetch_optional(&state.db)
    .await
    .map_err(anyhow::Error::from)?;
    let Some(site_id) = site_id else {
        return Ok(HttpResponse::NoContent().finish());
    };
    let deployment = deployment::enqueue(&state, &site_id).await?;
    Ok(HttpResponse::Accepted().json(serde_json::json!({"deployment_id": deployment.id})))
}

pub fn routes(config: &mut web::ServiceConfig) {
    config.route("/webhooks/github", web::post().to(github));
}
