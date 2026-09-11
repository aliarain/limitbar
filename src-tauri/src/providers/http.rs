//! Shared GET-JSON with provider-neutral error mapping. Never logs headers.

use crate::usage::models::ProviderError;
use std::time::Duration;

pub const USER_AGENT: &str = concat!("LimitBar/", env!("CARGO_PKG_VERSION"));
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn get_json<T: serde::de::DeserializeOwned>(
    req: reqwest::RequestBuilder,
    what: &str,
) -> Result<T, ProviderError> {
    let resp = req
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| {
            // reqwest errors never embed request headers, so this is safe to surface.
            ProviderError::Network(if e.is_timeout() { "timed out".into() } else if e.is_connect() { "connection failed".into() } else { "request failed".into() })
        })?;
    match resp.status().as_u16() {
        200..=299 => resp.json::<T>().await.map_err(|_| ProviderError::Parse(format!("invalid JSON from {what}"))),
        401 | 403 => Err(ProviderError::AuthRequired),
        429 => {
            let retry_after_secs = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            Err(ProviderError::RateLimited { retry_after_secs })
        }
        code => Err(ProviderError::Http { status: code }),
    }
}

/// Reads a small JSON credential file. Missing → AuthRequired; unreadable → Network.
pub async fn read_json_file<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T, ProviderError> {
    let raw = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(ProviderError::AuthRequired),
        Err(e) => return Err(ProviderError::Network(format!("cannot read credential file: {}", e.kind()))),
    };
    serde_json::from_slice(&raw).map_err(|_| ProviderError::Parse("credential file is not valid JSON".into()))
}

/// Bearer secret wrapper whose Debug never reveals the value.
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
