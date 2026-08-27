use std::{
    env,
    ffi::OsStr,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub secure_cookies: bool,
    pub chimney_bind: SocketAddr,
    pub chimney_https_port: Option<u16>,
    pub chimney_acme_email: Option<String>,
    pub release_retention: usize,
    pub expected_ips: Vec<IpAddr>,
    pub mise_bin: Option<PathBuf>,
    pub webhook_secret: Option<String>,
    pub public_url: Option<String>,
    pub base_domain: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind = env::var("BLANK_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8080".into())
            .parse()
            .context("BLANK_BIND must be a socket address")?;
        let mut data_dir = env::var_os("BLANK_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data"));
        if data_dir.is_relative() {
            data_dir = env::current_dir()
                .context("failed to resolve current directory")?
                .join(data_dir);
        }
        let secure_cookies = env::var("BLANK_SECURE_COOKIES")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let chimney_bind = env::var("BLANK_CHIMNEY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8081".into())
            .parse()
            .context("BLANK_CHIMNEY_BIND must be a socket address")?;
        let chimney_https_port = env::var("BLANK_CHIMNEY_HTTPS_PORT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                value
                    .parse()
                    .context("BLANK_CHIMNEY_HTTPS_PORT must be a port")
            })
            .transpose()?;
        let chimney_acme_email = env::var("BLANK_CHIMNEY_ACME_EMAIL")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let release_retention = env::var("BLANK_RELEASE_RETENTION")
            .unwrap_or_else(|_| "5".into())
            .parse::<usize>()
            .context("BLANK_RELEASE_RETENTION must be a positive integer")?;
        if release_retention == 0 {
            anyhow::bail!("BLANK_RELEASE_RETENTION must be at least 1");
        }
        let expected_ips = env::var("BLANK_EXPECTED_IPS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse()
                    .context("BLANK_EXPECTED_IPS contains an invalid IP address")
            })
            .collect::<Result<Vec<_>>>()?;
        let mise_bin = env::var_os("BLANK_MISE_BIN")
            .map(PathBuf::from)
            .or_else(|| find_on_path("mise"));
        let webhook_secret = env::var("BLANK_WEBHOOK_SECRET")
            .ok()
            .filter(|v| !v.is_empty());
        let public_url = env::var("BLANK_PUBLIC_URL")
            .ok()
            .map(|value| value.trim_end_matches('/').to_owned())
            .filter(|value| !value.is_empty());
        let base_domain = env::var("BLANK_BASE_DOMAIN")
            .ok()
            .map(|value| value.trim().trim_matches('.').to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        if base_domain.as_ref().is_some_and(|domain| {
            domain.len() > 253
                || !domain.contains('.')
                || domain.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                })
        }) {
            anyhow::bail!("BLANK_BASE_DOMAIN must be a valid base domain");
        }
        if chimney_https_port.is_some() && chimney_acme_email.is_none() {
            anyhow::bail!("BLANK_CHIMNEY_ACME_EMAIL is required when Chimney HTTPS is enabled");
        }

        Ok(Self {
            bind,
            data_dir,
            secure_cookies,
            chimney_bind,
            chimney_https_port,
            chimney_acme_email,
            release_retention,
            expected_ips,
            mise_bin,
            webhook_secret,
            public_url,
            base_domain,
        })
    }
}

fn find_on_path(program: impl AsRef<OsStr>) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(program.as_ref()))
            .find(|candidate| candidate.is_file())
    })
}
