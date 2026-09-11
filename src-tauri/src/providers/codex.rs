//! OpenAI Codex provider.
//!
//! Data source: `GET https://chatgpt.com/backend-api/wham/usage` — what the Codex CLI's
//! own `/status` limits display uses. See docs/provider-feasibility.md.
//! Credential: `~/.codex/auth.json` (written by `codex login`). Read per fetch,
//! never refreshed or written by LimitBar.

use super::http::{get_json, read_json_file, Secret};
use super::UsageProvider;
use crate::usage::models::{ProviderError, ProviderId, UsageSnapshot, UsageSource, UsageStatus, UsageWindow};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::path::PathBuf;

const API_BASE: &str = "https://chatgpt.com/backend-api";

#[derive(Debug, Deserialize)]
struct AuthFile {
    auth_mode: Option<String>,
    tokens: Option<Tokens>,
}

#[derive(Debug, Deserialize)]
struct Tokens {
    access_token: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageResponse {
    plan_type: Option<String>,
    rate_limit: Option<RateLimit>,
    #[serde(default)]
    additional_rate_limits: Vec<AdditionalLimit>,
    credits: Option<Credits>,
}

#[derive(Debug, Deserialize)]
struct RateLimit {
    limit_reached: Option<bool>,
    primary_window: Option<WireWindow>,
    secondary_window: Option<WireWindow>,
}

#[derive(Debug, Deserialize)]
struct WireWindow {
    used_percent: Option<f64>,
    limit_window_seconds: Option<i64>,
    /// Epoch seconds.
    reset_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AdditionalLimit {
    limit_name: Option<String>,
    rate_limit: Option<RateLimit>,
}

#[derive(Debug, Deserialize)]
struct Credits {
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    balance: Option<String>,
}

struct Creds {
    token: Secret,
    account_id: Option<String>,
}

pub struct CodexProvider {
    http: reqwest::Client,
    auth_path: PathBuf,
    base_url: String,
}

impl CodexProvider {
    pub fn new(http: reqwest::Client) -> Self {
        let auth_path = dirs::home_dir().unwrap_or_default().join(".codex").join("auth.json");
        Self { http, auth_path, base_url: API_BASE.to_string() }
    }

    #[cfg(test)]
    fn with_paths(http: reqwest::Client, auth_path: PathBuf, base_url: String) -> Self {
        Self { http, auth_path, base_url }
    }

    async fn load_creds(&self) -> Result<Creds, ProviderError> {
        let auth: AuthFile = read_json_file(&self.auth_path).await?;
        if auth.auth_mode.as_deref() == Some("apikey") {
            return Err(ProviderError::Unsupported("API-key mode has no subscription limits".into()));
        }
        let tokens = auth.tokens.ok_or(ProviderError::AuthRequired)?;
        match tokens.access_token {
            Some(t) if !t.trim().is_empty() => Ok(Creds { token: Secret(t), account_id: tokens.account_id }),
            _ => Err(ProviderError::AuthRequired),
        }
    }
}

#[async_trait]
impl UsageProvider for CodexProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn name(&self) -> &'static str {
        "Codex"
    }

    async fn get_usage(&self) -> Result<UsageSnapshot, ProviderError> {
        let creds = self.load_creds().await?;
        let mut req = self.http.get(format!("{}/wham/usage", self.base_url)).bearer_auth(&creds.token.0);
        if let Some(acct) = creds.account_id.as_deref() {
            req = req.header("ChatGPT-Account-Id", acct);
        }
        let usage: UsageResponse = get_json(req, "wham/usage").await?;
        Ok(normalize(usage, Utc::now()))
    }
}

// ---- normalization (pure; unit-tested against fixtures)

fn window_label(seconds: Option<i64>) -> String {
    match seconds {
        Some(18_000) => "5h".into(),
        Some(604_800) => "Week".into(),
        Some(86_400) => "Day".into(),
        Some(s) if s > 0 && s % 3600 == 0 => format!("{}h", s / 3600),
        _ => "Limit".into(),
    }
}

fn window_from_wire(id: &str, label: &str, w: &WireWindow, limit_reached: bool) -> Option<UsageWindow> {
    let used = w.used_percent?;
    let mut win = UsageWindow::from_used_cap(id, label, used, 100.0);
    win.reset_at = w.reset_at.filter(|s| *s > 0).and_then(|s| Utc.timestamp_opt(s, 0).single());
    win.exceeded = limit_reached && used >= 100.0;
    Some(win)
}

fn plan_display_name(plan: &str) -> String {
    match plan {
        "free" => "Free".into(),
        "plus" => "Plus".into(),
        "pro" => "Pro".into(),
        "team" => "Team".into(),
        "business" => "Business".into(),
        "enterprise" => "Enterprise".into(),
        "edu" => "Edu".into(),
        other => other.into(),
    }
}

pub(crate) fn normalize(usage: UsageResponse, now: DateTime<Utc>) -> UsageSnapshot {
    let mut windows = Vec::new();

    if let Some(rl) = usage.rate_limit.as_ref() {
        let reached = rl.limit_reached.unwrap_or(false);
        for (id, w) in [("primary", rl.primary_window.as_ref()), ("secondary", rl.secondary_window.as_ref())] {
            if let Some(w) = w {
                let label = window_label(w.limit_window_seconds);
                if let Some(win) = window_from_wire(id, &label, w, reached) {
                    windows.push(win);
                }
            }
        }
    }
    // Shortest window first so the 5h meter is primary when present.
    windows.sort_by_key(|w| match w.label.as_str() { "5h" => 0, "Day" => 1, "Week" => 2, _ => 3 });

    // Model-specific limits: only surface ones with usage, to keep the card small.
    for extra in usage.additional_rate_limits.iter() {
        let Some(rl) = extra.rate_limit.as_ref() else { continue };
        let name = extra.limit_name.clone().unwrap_or_else(|| "Model".into());
        let reached = rl.limit_reached.unwrap_or(false);
        for (suffix, w) in [("primary", rl.primary_window.as_ref()), ("secondary", rl.secondary_window.as_ref())] {
            if let Some(w) = w {
                if w.used_percent.unwrap_or(0.0) <= 0.0 {
                    continue;
                }
                let label = format!("{name} {}", window_label(w.limit_window_seconds));
                let id = format!("extra:{name}:{suffix}");
                if let Some(mut win) = window_from_wire(&id, &label, w, reached) {
                    win.scoped = true;
                    windows.push(win);
                }
            }
        }
    }

    let (status, message) = if windows.is_empty() {
        (UsageStatus::Unavailable, Some("Codex did not report any usage windows.".into()))
    } else {
        (UsageStatus::Available, None)
    };

    let detail = usage.credits.as_ref().and_then(|c| {
        if c.unlimited == Some(true) {
            Some("unlimited credits".to_string())
        } else if c.has_credits == Some(true) {
            c.balance.as_ref().map(|b| format!("{b} credits available"))
        } else {
            None
        }
    });

    UsageSnapshot {
        provider_id: ProviderId::Codex,
        provider_name: "Codex".into(),
        status,
        source: UsageSource::OfficialApi,
        windows,
        plan_name: usage.plan_type.as_deref().map(plan_display_name),
        account_identifier: None,
        detail,
        message,
        fetched_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/codex").join(name)).expect("fixture")
    }

    #[test]
    fn normalizes_verified_live_response() {
        let u: UsageResponse = serde_json::from_str(&fixture("usage.json")).unwrap();
        let s = normalize(u, Utc::now());
        assert_eq!(s.status, UsageStatus::Available);
        assert_eq!(s.plan_name.as_deref(), Some("Pro"));
        assert_eq!(s.windows.len(), 1, "zero-usage model limits are not surfaced");
        let w = &s.windows[0];
        assert_eq!(w.label, "Week");
        assert_eq!(w.used_percent, Some(66.0));
        assert_eq!(w.remaining_percent, Some(34.0));
        assert_eq!(w.reset_at.unwrap().to_rfc3339(), "2026-09-15T01:28:01+00:00");
        assert!(!w.exceeded);
        assert_eq!(s.detail, None);
    }

    #[test]
    fn two_windows_sorted_and_exceeded_marked() {
        let u: UsageResponse = serde_json::from_str(&fixture("usage-two-windows.json")).unwrap();
        let s = normalize(u, Utc::now());
        assert_eq!(s.plan_name.as_deref(), Some("Plus"));
        let labels: Vec<&str> = s.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["5h", "Week", "Spark Week"]);
        assert!(s.windows[0].exceeded);
        assert_eq!(s.windows[0].remaining_percent, Some(0.0));
        assert!(!s.windows[1].exceeded);
        assert_eq!(s.windows[2].used_percent, Some(12.0));
        assert!(s.windows[2].scoped);
        assert_eq!(s.min_remaining_percent(), Some(0.0));
        assert_eq!(s.detail.as_deref(), Some("12.50 credits available"));
    }

    #[test]
    fn empty_response_is_unavailable() {
        let u: UsageResponse = serde_json::from_str("{}").unwrap();
        let s = normalize(u, Utc::now());
        assert_eq!(s.status, UsageStatus::Unavailable);
        assert!(s.windows.is_empty());
        assert_eq!(s.min_remaining_percent(), None);
    }

    #[test]
    fn window_labels() {
        assert_eq!(window_label(Some(18_000)), "5h");
        assert_eq!(window_label(Some(604_800)), "Week");
        assert_eq!(window_label(Some(7_200)), "2h");
        assert_eq!(window_label(None), "Limit");
    }

    #[test]
    fn auth_fixture_parses() {
        let a: AuthFile = serde_json::from_str(&fixture("auth.json")).unwrap();
        assert_eq!(a.auth_mode.as_deref(), Some("chatgpt"));
        assert_eq!(a.tokens.unwrap().account_id.as_deref(), Some("acct-redacted"));
    }

    #[tokio::test]
    async fn missing_auth_is_auth_required_and_apikey_mode_unsupported() {
        let p = CodexProvider::with_paths(reqwest::Client::new(), PathBuf::from("/nonexistent/codex/auth.json"), "http://127.0.0.1:9".into());
        assert!(matches!(p.get_usage().await, Err(ProviderError::AuthRequired)));

        let dir = std::env::temp_dir().join(format!("limitbar-codex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("auth.json");
        std::fs::write(&path, r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-SECRET"}"#).unwrap();
        let p = CodexProvider::with_paths(reqwest::Client::new(), path, "http://127.0.0.1:9".into());
        let err = p.get_usage().await.unwrap_err();
        assert!(matches!(err, ProviderError::Unsupported(_)));
        assert!(!err.to_string().contains("SECRET"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// cargo test --lib live_codex -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_codex() {
        let s = CodexProvider::new(reqwest::Client::new()).get_usage().await.expect("live fetch");
        println!("{s:#?}");
        assert_eq!(s.provider_id, ProviderId::Codex);
    }
}
