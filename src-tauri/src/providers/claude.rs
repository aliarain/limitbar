//! Claude Code provider.
//!
//! Data source: `GET https://api.anthropic.com/api/oauth/usage` — the endpoint behind
//! the CLI's `/usage`. See docs/provider-feasibility.md.
//! Credential: Claude Code's OAuth token — macOS Keychain item
//! `Claude Code-credentials` (account = current user), or `~/.claude/.credentials.json`
//! elsewhere. Read per fetch, never refreshed or written by LimitBar.

use super::http::{get_json, read_json_file, Secret};
use super::UsageProvider;
use crate::usage::models::{ProviderError, ProviderId, UsageSnapshot, UsageSource, UsageStatus, UsageWindow};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::path::PathBuf;

const API_BASE: &str = "https://api.anthropic.com";
const OAUTH_BETA: &str = "oauth-2025-04-20";
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OauthCreds>,
}

#[derive(Debug, Deserialize)]
struct OauthCreds {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    /// Epoch milliseconds.
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier")]
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageResponse {
    five_hour: Option<Bucket>,
    seven_day: Option<Bucket>,
    #[serde(default)]
    limits: Vec<Limit>,
}

#[derive(Debug, Deserialize)]
struct Bucket {
    utilization: Option<f64>,
    resets_at: Option<DateTime<Utc>>,
    locked_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Limit {
    group: Option<String>,
    percent: Option<f64>,
    severity: Option<String>,
    resets_at: Option<DateTime<Utc>>,
    scope: Option<Scope>,
}

#[derive(Debug, Deserialize)]
struct Scope {
    model: Option<ScopeModel>,
}

#[derive(Debug, Deserialize)]
struct ScopeModel {
    display_name: Option<String>,
}

/// Non-sensitive account context read alongside the token.
#[derive(Debug, Default, Clone)]
pub(crate) struct AccountContext {
    subscription_type: Option<String>,
    rate_limit_tier: Option<String>,
}

pub struct ClaudeProvider {
    http: reqwest::Client,
    /// Used on non-macOS, and as a fallback when the Keychain has no item.
    credentials_path: PathBuf,
    base_url: String,
    #[cfg(test)]
    skip_keychain: bool,
}

impl ClaudeProvider {
    pub fn new(http: reqwest::Client) -> Self {
        let credentials_path = dirs::home_dir().unwrap_or_default().join(".claude").join(".credentials.json");
        Self {
            http,
            credentials_path,
            base_url: API_BASE.to_string(),
            #[cfg(test)]
            skip_keychain: true,
        }
    }

    #[cfg(test)]
    fn with_paths(http: reqwest::Client, credentials_path: PathBuf, base_url: String) -> Self {
        Self { http, credentials_path, base_url, skip_keychain: true }
    }

    async fn load_credentials(&self) -> Result<(Secret, AccountContext), ProviderError> {
        let file = match self.read_keychain()? {
            Some(raw) => serde_json::from_slice::<CredentialsFile>(&raw)
                .map_err(|_| ProviderError::Parse("keychain credential is not valid JSON".into()))?,
            None => read_json_file::<CredentialsFile>(&self.credentials_path).await?,
        };
        let oauth = file.claude_ai_oauth.ok_or(ProviderError::AuthRequired)?;
        let token = match oauth.access_token {
            Some(t) if !t.trim().is_empty() => t,
            _ => return Err(ProviderError::AuthRequired),
        };
        if let Some(exp) = oauth.expires_at {
            if Utc.timestamp_millis_opt(exp).single().is_some_and(|t| t <= Utc::now()) {
                return Err(ProviderError::AuthRequired);
            }
        }
        Ok((
            Secret(token),
            AccountContext { subscription_type: oauth.subscription_type, rate_limit_tier: oauth.rate_limit_tier },
        ))
    }

    #[cfg(target_os = "macos")]
    fn read_keychain(&self) -> Result<Option<Vec<u8>>, ProviderError> {
        #[cfg(test)]
        if self.skip_keychain {
            return Ok(None);
        }
        let account = std::env::var("USER").map_err(|_| ProviderError::Network("cannot determine current user".into()))?;
        match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, &account) {
            Ok(bytes) => Ok(Some(bytes)),
            // errSecItemNotFound: not logged in via Claude Code on this machine.
            Err(e) if e.code() == -25300 => Ok(None),
            // errSecUserCanceled / errSecAuthFailed: the user denied LimitBar access.
            Err(e) if e.code() == -128 || e.code() == -25293 => Err(ProviderError::AuthRequired),
            Err(e) => Err(ProviderError::Network(format!("keychain error {}", e.code()))),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn read_keychain(&self) -> Result<Option<Vec<u8>>, ProviderError> {
        Ok(None)
    }
}

#[async_trait]
impl UsageProvider for ClaudeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    async fn get_usage(&self) -> Result<UsageSnapshot, ProviderError> {
        let (token, account) = self.load_credentials().await?;
        let req = self
            .http
            .get(format!("{}/api/oauth/usage", self.base_url))
            .bearer_auth(&token.0)
            .header("anthropic-beta", OAUTH_BETA);
        let usage: UsageResponse = get_json(req, "oauth/usage").await?;
        Ok(normalize(usage, &account, Utc::now()))
    }
}

// ---- normalization (pure; unit-tested against fixtures)

fn plan_display_name(account: &AccountContext) -> Option<String> {
    let base = match account.subscription_type.as_deref() {
        Some("max") => "Max",
        Some("pro") => "Pro",
        Some("team") => "Team",
        Some("enterprise") => "Enterprise",
        Some(other) => other,
        None => return None,
    };
    let multiplier = account
        .rate_limit_tier
        .as_deref()
        .and_then(|t| t.rsplit('_').next())
        .filter(|s| s.ends_with('x') && s[..s.len() - 1].chars().all(|c| c.is_ascii_digit()));
    Some(match multiplier {
        Some(m) => format!("{base} {m}"),
        None => base.to_string(),
    })
}

fn bucket_window(id: &str, label: &str, b: &Bucket) -> Option<UsageWindow> {
    let used = b.utilization?;
    let mut w = UsageWindow::from_used_cap(id, label, used, 100.0);
    w.reset_at = b.resets_at;
    w.exceeded = b.locked_reason.is_some() || used >= 100.0;
    Some(w)
}

pub(crate) fn normalize(usage: UsageResponse, account: &AccountContext, now: DateTime<Utc>) -> UsageSnapshot {
    let mut windows = Vec::new();
    if let Some(w) = usage.five_hour.as_ref().and_then(|b| bucket_window("five_hour", "5h", b)) {
        windows.push(w);
    }
    if let Some(w) = usage.seven_day.as_ref().and_then(|b| bucket_window("seven_day", "Week", b)) {
        windows.push(w);
    }
    // Model-scoped weekly caps (e.g. a per-model Opus limit) — only when they carry usage.
    for l in usage.limits.iter() {
        let Some(name) = l.scope.as_ref().and_then(|s| s.model.as_ref()).and_then(|m| m.display_name.clone()) else { continue };
        if l.group.as_deref() != Some("weekly") {
            continue;
        }
        let Some(pct) = l.percent.filter(|p| *p > 0.0) else { continue };
        let mut w = UsageWindow::from_used_cap(&format!("model:{name}"), &format!("{name} Week"), pct, 100.0);
        w.reset_at = l.resets_at;
        w.exceeded = l.severity.as_deref() == Some("critical") || pct >= 100.0;
        w.scoped = true;
        windows.push(w);
    }

    let (status, message) = if windows.is_empty() {
        (UsageStatus::Unavailable, Some("Claude did not report any usage windows.".into()))
    } else {
        (UsageStatus::Available, None)
    };

    UsageSnapshot {
        provider_id: ProviderId::Claude,
        provider_name: "Claude Code".into(),
        status,
        source: UsageSource::OfficialApi,
        windows,
        plan_name: plan_display_name(account),
        account_identifier: None,
        detail: None,
        message,
        fetched_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/claude").join(name)).expect("fixture")
    }

    fn account() -> AccountContext {
        AccountContext { subscription_type: Some("max".into()), rate_limit_tier: Some("default_claude_max_20x".into()) }
    }

    #[test]
    fn normalizes_verified_live_response() {
        let u: UsageResponse = serde_json::from_str(&fixture("usage.json")).unwrap();
        let s = normalize(u, &account(), Utc::now());
        assert_eq!(s.status, UsageStatus::Available);
        assert_eq!(s.plan_name.as_deref(), Some("Max 20x"));
        let labels: Vec<&str> = s.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["5h", "Week", "Fable Week"]);
        assert_eq!(s.windows[0].remaining_percent, Some(97.0));
        assert_eq!(s.windows[0].reset_at.unwrap().to_rfc3339_opts(chrono::SecondsFormat::Secs, true), "2026-09-11T14:00:00Z");
        assert_eq!(s.windows[1].remaining_percent, Some(39.0));
        assert!(s.windows[2].exceeded);
        assert!(s.windows[2].scoped);
        assert_eq!(s.windows[2].remaining_percent, Some(0.0));
        // Headline follows the account-level weekly (39%), not the exhausted model cap.
        assert_eq!(s.min_remaining_percent(), Some(39.0));
        assert_eq!(s.next_reset_at(), s.windows[0].reset_at);
    }

    #[test]
    fn locked_bucket_is_exceeded() {
        let u: UsageResponse = serde_json::from_str(&fixture("usage-locked.json")).unwrap();
        let s = normalize(u, &account(), Utc::now());
        assert_eq!(s.windows.len(), 2);
        assert!(s.windows[0].exceeded);
        assert_eq!(s.windows[0].remaining_percent, Some(0.0));
        assert!(!s.windows[1].exceeded);
    }

    #[test]
    fn plan_names() {
        assert_eq!(plan_display_name(&account()).as_deref(), Some("Max 20x"));
        assert_eq!(plan_display_name(&AccountContext { subscription_type: Some("pro".into()), rate_limit_tier: Some("default_claude_pro".into()) }).as_deref(), Some("Pro"));
        assert_eq!(plan_display_name(&AccountContext { subscription_type: Some("max".into()), rate_limit_tier: None }).as_deref(), Some("Max"));
        assert_eq!(plan_display_name(&AccountContext::default()), None);
    }

    #[test]
    fn empty_is_unavailable() {
        let u: UsageResponse = serde_json::from_str("{}").unwrap();
        let s = normalize(u, &AccountContext::default(), Utc::now());
        assert_eq!(s.status, UsageStatus::Unavailable);
        assert_eq!(s.min_remaining_percent(), None);
    }

    #[test]
    fn credentials_fixture_parses() {
        let c: CredentialsFile = serde_json::from_str(&fixture("credentials.json")).unwrap();
        let o = c.claude_ai_oauth.unwrap();
        assert_eq!(o.subscription_type.as_deref(), Some("max"));
        assert!(o.access_token.is_some());
    }

    #[tokio::test]
    async fn expired_token_is_auth_required_without_leaking() {
        let dir = std::env::temp_dir().join(format!("limitbar-claude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".credentials.json");
        std::fs::write(&path, r#"{"claudeAiOauth":{"accessToken":"sk-ant-SECRET","expiresAt":1000}}"#).unwrap();
        let p = ClaudeProvider::with_paths(reqwest::Client::new(), path.clone(), "http://127.0.0.1:9".into());
        let err = p.get_usage().await.unwrap_err();
        assert!(matches!(err, ProviderError::AuthRequired), "{err}");
        assert!(!err.to_string().contains("SECRET"));

        // Valid-looking token but unreachable server → network error, still no leak.
        std::fs::write(&path, r#"{"claudeAiOauth":{"accessToken":"sk-ant-SECRET","expiresAt":4102444800000}}"#).unwrap();
        let err = p.get_usage().await.unwrap_err();
        assert!(matches!(err, ProviderError::Network(_)), "{err}");
        assert!(!err.to_string().contains("SECRET"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn missing_credentials_is_auth_required() {
        let p = ClaudeProvider::with_paths(reqwest::Client::new(), PathBuf::from("/nonexistent/claude/.credentials.json"), "http://127.0.0.1:9".into());
        assert!(matches!(p.get_usage().await, Err(ProviderError::AuthRequired)));
    }

    /// cargo test --lib live_claude -- --ignored --nocapture   (may prompt for Keychain access)
    #[tokio::test]
    #[ignore]
    async fn live_claude() {
        let mut p = ClaudeProvider::new(reqwest::Client::new());
        p.skip_keychain = false;
        let s = p.get_usage().await.expect("live fetch");
        println!("{s:#?}");
        assert_eq!(s.provider_id, ProviderId::Claude);
    }
}
