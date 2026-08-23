use std::net::IpAddr;

use actix_web::{HttpRequest, HttpResponse, web};
use anyhow::Context;
use hickory_resolver::{
    Resolver,
    proto::rr::{RData, RecordType},
};
use serde::{Deserialize, Serialize};

use crate::{auth::require_session, error::ApiError, state::AppState};

#[derive(Deserialize)]
struct CheckRequest {
    domains: Vec<String>,
}

#[derive(Serialize)]
struct CheckResult {
    domain: String,
    addresses: Vec<IpAddr>,
    canonical_names: Vec<String>,
    matches_expected: Option<bool>,
    error: Option<String>,
}

async fn check(
    req: HttpRequest,
    state: web::Data<AppState>,
    input: web::Json<CheckRequest>,
) -> Result<HttpResponse, ApiError> {
    require_session(&req, &state.db, true).await?;
    if input.domains.is_empty() || input.domains.len() > 20 {
        return Err(ApiError::BadRequest(
            "check between 1 and 20 domains".into(),
        ));
    }
    let resolver = Resolver::builder_tokio()
        .context("failed to load system DNS configuration")?
        .build()
        .context("failed to initialize DNS resolver")?;
    let mut results = Vec::with_capacity(input.domains.len());
    for raw in &input.domains {
        let domain = raw.trim().trim_end_matches('.').to_ascii_lowercase();
        if !valid_domain(&domain) {
            results.push(CheckResult {
                domain,
                addresses: vec![],
                canonical_names: vec![],
                matches_expected: None,
                error: Some("invalid domain".into()),
            });
            continue;
        }
        let fqdn = format!("{domain}.");
        let address_result = resolver.lookup_ip(fqdn.clone()).await;
        let addresses: Vec<IpAddr> = address_result
            .as_ref()
            .map(|lookup| lookup.iter().collect())
            .unwrap_or_default();
        let canonical_names = resolver
            .lookup(fqdn, RecordType::CNAME)
            .await
            .map(|lookup| {
                lookup
                    .answers()
                    .iter()
                    .filter_map(|record| match &record.data {
                        RData::CNAME(name) => {
                            Some(name.to_utf8().trim_end_matches('.').to_string())
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let matches_expected = (!state.config.expected_ips.is_empty()).then(|| {
            addresses
                .iter()
                .any(|address| state.config.expected_ips.contains(address))
        });
        let error = address_result
            .err()
            .map(|error| concise_error(&error.to_string()));
        results.push(CheckResult {
            domain,
            addresses,
            canonical_names,
            matches_expected,
            error,
        });
    }
    Ok(HttpResponse::Ok().json(results))
}

pub(crate) fn valid_domain(domain: &str) -> bool {
    domain.len() <= 253
        && domain.contains('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn concise_error(error: &str) -> String {
    if error.to_ascii_lowercase().contains("no record")
        || error.to_ascii_lowercase().contains("nxdomain")
    {
        "no public A or AAAA records found".into()
    } else {
        "DNS lookup failed".into()
    }
}

pub fn routes(config: &mut web::ServiceConfig) {
    config.route("/dns/check", web::post().to(check));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_host_names() {
        assert!(valid_domain("www.example.com"));
        assert!(!valid_domain("https://example.com"));
        assert!(!valid_domain("-bad.example.com"));
    }
}
