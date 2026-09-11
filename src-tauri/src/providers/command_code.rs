//! Command Code provider.
//!
//! Data source: `GET https://api.commandcode.ai/alpha/billing/credits` — the same
//! endpoint the vendor's CLI and desktop app use. See docs/provider-feasibility.md
//! for the verified contract, the ToS review and the good-citizen policy.
//!
//! Credential: `~/.commandcode/auth.json` → `apiKey`, written by `cmd login` or the
//! desktop app. Read into memory per fetch, never stored, never logged.

use super::http::{get_json, read_json_file, Secret as ApiKey};
use super::UsageProvider;
use crate::usage::models::{
    ProviderError, ProviderId, UsageSnapshot, UsageSource, UsageStatus, UsageWindow,
};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::path::PathBuf;

const API_BASE: &str = "https://api.commandcode.ai";

// ---- wire types (tolerant: unknown fields ignored, optional where the CLI's own schema is optional)

#[derive(Debug, Deserialize)]
struct AuthFile {
    #[serde(rename = "apiKey")]
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhoamiResponse {
    user: Option<WhoamiUser>,
    org: Option<WhoamiOrg>,
}

#[derive(Debug, Deserialize)]
struct WhoamiUser {
    #[serde(rename = "userName")]
    user_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhoamiOrg {
    id: Option<String>,
    login: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionsResponse {
    data: Option<SubscriptionData>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionData {
    status: Option<String>,
    #[serde(rename = "planId")]
    plan_id: Option<String>,
    #[serde(rename = "currentPeriodEnd")]
    current_period_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreditsResponse {
    credits: Option<Credits>,
    #[serde(rename = "windowLimits")]
    window_limits: Option<WindowLimits>,
}

#[derive(Debug, Deserialize)]
struct Credits {
    #[serde(rename = "monthlyCredits")]
    monthly_credits: Option<f64>,
    #[serde(rename = "purchasedCredits")]
    purchased_credits: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct WindowLimits {
    limited: Option<bool>,
    #[serde(rename = "fiveHour")]
    five_hour: Option<WireWindow>,
    weekly: Option<WireWindow>,
}

#[derive(Debug, Deserialize)]
struct WireWindow {
    used: Option<f64>,
    cap: Option<f64>,
    exceeded: Option<bool>,
    /// Epoch milliseconds. `0` means no window is currently open.
    #[serde(rename = "resetAt")]
    reset_at: Option<i64>,
}

/// Non-sensitive account context fetched alongside credits.
#[derive(Debug, Default, Clone)]
pub(crate) struct AccountContext {
    org_id: Option<String>,
    account_identifier: Option<String>,
    plan_id: Option<String>,
    subscription_status: Option<String>,
    period_end: Option<DateTime<Utc>>,
}

pub struct CommandCodeProvider {
    http: reqwest::Client,
    auth_path: PathBuf,
    base_url: String,
}

impl CommandCodeProvider {
    pub fn new(http: reqwest::Client) -> Self {
        let auth_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".commandcode")
            .join("auth.json");
        Self { http, auth_path, base_url: API_BASE.to_string() }
    }

    #[cfg(test)]
    fn with_paths(http: reqwest::Client, auth_path: PathBuf, base_url: String) -> Self {
        Self { http, auth_path, base_url }
    }

    async fn load_api_key(&self) -> Result<ApiKey, ProviderError> {
        let parsed: AuthFile = read_json_file(&self.auth_path).await?;
        match parsed.api_key {
            Some(k) if !k.trim().is_empty() => Ok(ApiKey(k)),
            _ => Err(ProviderError::AuthRequired),
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        key: &ApiKey,
        path: &str,
        org_id: Option<&str>,
    ) -> Result<T, ProviderError> {
        let mut req = self.http.get(format!("{}{}", self.base_url, path)).bearer_auth(&key.0);
        if let Some(org) = org_id {
            req = req.query(&[("orgId", org)]);
        }
        get_json(req, path).await
    }

    async fn fetch_account(&self, key: &ApiKey) -> Result<AccountContext, ProviderError> {
        let who: WhoamiResponse = self.get_json(key, "/alpha/whoami", None).await?;
        let org_id = who.org.as_ref().and_then(|o| o.id.clone());
        let account_identifier = who
            .org
            .as_ref()
            .and_then(|o| o.login.clone())
            .or_else(|| who.user.as_ref().and_then(|u| u.user_name.clone()));

        let subs: SubscriptionsResponse = self
            .get_json(key, "/alpha/billing/subscriptions", org_id.as_deref())
            .await?;
        let data = subs.data;
        Ok(AccountContext {
            org_id,
            account_identifier,
            plan_id: data.as_ref().and_then(|d| d.plan_id.clone()),
            subscription_status: data.as_ref().and_then(|d| d.status.clone()),
            period_end: data.as_ref().and_then(|d| d.current_period_end),
        })
    }
}

#[async_trait]
impl UsageProvider for CommandCodeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::CommandCode
    }

    fn name(&self) -> &'static str {
        "Command Code"
    }

    async fn get_usage(&self) -> Result<UsageSnapshot, ProviderError> {
        let key = self.load_api_key().await?;
        let account = self.fetch_account(&key).await?;
        let credits: CreditsResponse = self
            .get_json(&key, "/alpha/billing/credits", account.org_id.as_deref())
            .await?;
        Ok(normalize(credits, &account, Utc::now()))
    }
}

// ---- normalization (pure; unit-tested against fixtures)

/// Display names only. Unknown plan ids pass through verbatim; this table is
/// never used for arithmetic (see feasibility doc: the CLI's plan table is stale).
fn plan_display_name(plan_id: &str) -> String {
    match plan_id {
        "individual-go" => "Go".into(),
        "individual-goat" => "GOAT".into(),
        "individual-pro" => "Pro".into(),
        "individual-max" => "Max".into(),
        "individual-ultra" => "Ultra".into(),
        "teams-pro" => "Teams Pro".into(),
        other => other.into(),
    }
}

fn window_from_wire(id: &str, label: &str, w: &WireWindow) -> Option<UsageWindow> {
    let (used, cap) = (w.used?, w.cap?);
    let mut win = UsageWindow::from_used_cap(id, label, used, cap);
    win.exceeded = w.exceeded.unwrap_or(false);
    // 0 (or negative) means "no window open" — there is nothing to count down to.
    win.reset_at = w
        .reset_at
        .filter(|ms| *ms > 0)
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single());
    Some(win)
}

fn format_credits(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

pub(crate) fn normalize(
    credits: CreditsResponse,
    account: &AccountContext,
    now: DateTime<Utc>,
) -> UsageSnapshot {
    let mut windows = Vec::new();
    let mut status = UsageStatus::Available;
    let mut message = None;

    match credits.window_limits.as_ref() {
        Some(wl) if wl.limited == Some(false) => {
            status = UsageStatus::Unavailable;
            message = Some("This account has no rolling usage caps.".into());
        }
        Some(wl) => {
            if let Some(w) = wl.five_hour.as_ref().and_then(|w| window_from_wire("five_hour", "5h", w)) {
                windows.push(w);
            }
            if let Some(w) = wl.weekly.as_ref().and_then(|w| window_from_wire("weekly", "Week", w)) {
                windows.push(w);
            }
            if windows.is_empty() {
                status = UsageStatus::Unavailable;
                message = Some("Usage windows not reported by Command Code.".into());
            }
        }
        None => {
            status = UsageStatus::Unavailable;
            message = Some("Usage windows not reported by Command Code.".into());
        }
    }

    if let Some(s) = account.subscription_status.as_deref() {
        if s != "active" && s != "past_due" && s != "trialing" {
            status = UsageStatus::Unavailable;
            message = Some(format!("Subscription is {s}."));
        }
    }

    let mut detail_parts = Vec::new();
    if let Some(c) = credits.credits.as_ref() {
        let monthly = c.monthly_credits.unwrap_or(0.0).max(0.0);
        let purchased = c.purchased_credits.unwrap_or(0.0).max(0.0);
        let mut s = format!("{} credits left", format_credits(monthly));
        if purchased > 0.0 {
            s.push_str(&format!(" (+{} extra)", format_credits(purchased)));
        }
        detail_parts.push(s);
    }
    if let Some(end) = account.period_end {
        detail_parts.push(format!("period ends {}", end.format("%b %-d")));
    }

    UsageSnapshot {
        provider_id: ProviderId::CommandCode,
        provider_name: "Command Code".into(),
        status,
        source: UsageSource::OfficialApi,
        windows,
        plan_name: account.plan_id.as_deref().map(plan_display_name),
        account_identifier: account.account_identifier.clone(),
        detail: if detail_parts.is_empty() { None } else { Some(detail_parts.join(" · ")) },
        message,
        fetched_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/command-code")
            .join(name);
        std::fs::read_to_string(p).expect("fixture")
    }

    fn account() -> AccountContext {
        AccountContext {
            org_id: None,
            account_identifier: Some("example".into()),
            plan_id: Some("individual-goat".into()),
            subscription_status: Some("active".into()),
            period_end: Some(Utc.with_ymd_and_hms(2026, 10, 5, 16, 19, 13).unwrap()),
        }
    }

    #[test]
    fn normalizes_verified_live_response() {
        let credits: CreditsResponse = serde_json::from_str(&fixture("credits.json")).unwrap();
        let s = normalize(credits, &account(), Utc::now());

        assert_eq!(s.status, UsageStatus::Available);
        assert_eq!(s.source, UsageSource::OfficialApi);
        assert_eq!(s.plan_name.as_deref(), Some("GOAT"));
        assert_eq!(s.account_identifier.as_deref(), Some("example"));
        assert_eq!(s.detail.as_deref(), Some("67.7 credits left · period ends Oct 5"));
        assert_eq!(s.windows.len(), 2);

        let five = &s.windows[0];
        assert_eq!(five.id, "five_hour");
        assert_eq!(five.used_percent, Some(0.0));
        assert_eq!(five.remaining_percent, Some(100.0));
        assert_eq!(five.reset_at, None, "resetAt 0 must mean no open window");
        assert!(!five.exceeded);

        let week = &s.windows[1];
        assert_eq!(week.id, "weekly");
        assert!((week.used_percent.unwrap() - 6.669).abs() < 0.01);
        assert_eq!(
            week.reset_at.unwrap().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "2026-09-14T22:21:32.261Z"
        );
        assert_eq!(s.next_reset_at(), week.reset_at);
        assert!((s.min_remaining_percent().unwrap() - 93.331).abs() < 0.01);
    }

    #[test]
    fn active_five_hour_and_exceeded_weekly() {
        let credits: CreditsResponse = serde_json::from_str(&fixture("credits-active-5h.json")).unwrap();
        let s = normalize(credits, &account(), Utc::now());
        assert_eq!(s.status, UsageStatus::Available);
        assert!((s.windows[0].used_percent.unwrap() - 70.0).abs() < 0.01);
        assert!(s.windows[0].reset_at.is_some());
        assert_eq!(s.windows[1].used_percent, Some(100.0));
        assert!(s.windows[1].exceeded);
        assert_eq!(s.detail.as_deref(), Some("40.5 credits left (+12 extra) · period ends Oct 5"));
    }

    #[test]
    fn unlimited_account_is_unavailable_not_zero() {
        let credits: CreditsResponse = serde_json::from_str(&fixture("credits-unlimited.json")).unwrap();
        let s = normalize(credits, &account(), Utc::now());
        assert_eq!(s.status, UsageStatus::Unavailable);
        assert!(s.windows.is_empty());
        assert_eq!(s.min_remaining_percent(), None);
        assert!(s.message.is_some());
    }

    #[test]
    fn missing_window_limits_is_unavailable() {
        let credits: CreditsResponse = serde_json::from_str(&fixture("credits-no-windows.json")).unwrap();
        let s = normalize(credits, &account(), Utc::now());
        assert_eq!(s.status, UsageStatus::Unavailable);
        assert!(s.windows.is_empty());
        assert_eq!(s.detail.as_deref(), Some("5 credits left · period ends Oct 5"));
    }

    #[test]
    fn unknown_plan_passes_through_and_canceled_sub_is_unavailable() {
        let credits: CreditsResponse = serde_json::from_str(&fixture("credits.json")).unwrap();
        let mut acct = account();
        acct.plan_id = Some("individual-new-plan".into());
        acct.subscription_status = Some("canceled".into());
        let s = normalize(credits, &acct, Utc::now());
        assert_eq!(s.plan_name.as_deref(), Some("individual-new-plan"));
        assert_eq!(s.status, UsageStatus::Unavailable);
        assert_eq!(s.message.as_deref(), Some("Subscription is canceled."));
    }

    #[test]
    fn wire_types_parse_fixtures() {
        let _: WhoamiResponse = serde_json::from_str(&fixture("whoami.json")).unwrap();
        let org: WhoamiResponse = serde_json::from_str(&fixture("whoami-org.json")).unwrap();
        assert_eq!(org.org.unwrap().id.as_deref(), Some("org-redacted"));
        let subs: SubscriptionsResponse = serde_json::from_str(&fixture("subscriptions.json")).unwrap();
        assert_eq!(subs.data.unwrap().plan_id.as_deref(), Some("individual-goat"));
    }

    #[test]
    fn api_key_debug_is_redacted() {
        let k = ApiKey("user_supersecretvalue".into());
        assert_eq!(format!("{k:?}"), "Secret(<redacted>)");
    }

    #[tokio::test]
    async fn missing_auth_file_is_auth_required() {
        let p = CommandCodeProvider::with_paths(
            reqwest::Client::new(),
            PathBuf::from("/nonexistent/limitbar/auth.json"),
            "http://127.0.0.1:9".into(),
        );
        assert!(matches!(p.get_usage().await, Err(ProviderError::AuthRequired)));
    }

    #[tokio::test]
    async fn empty_key_is_auth_required() {
        let dir = std::env::temp_dir().join(format!("limitbar-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("auth.json");
        std::fs::write(&path, r#"{"apiKey":"  ","userName":"x"}"#).unwrap();
        let p = CommandCodeProvider::with_paths(reqwest::Client::new(), path.clone(), "http://127.0.0.1:9".into());
        assert!(matches!(p.get_usage().await, Err(ProviderError::AuthRequired)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn network_failure_is_network_error_without_secrets() {
        let dir = std::env::temp_dir().join(format!("limitbar-test-net-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("auth.json");
        std::fs::write(&path, r#"{"apiKey":"user_SECRET_VALUE"}"#).unwrap();
        // Port 9 (discard) — connection refused immediately.
        let p = CommandCodeProvider::with_paths(reqwest::Client::new(), path.clone(), "http://127.0.0.1:9".into());
        let err = p.get_usage().await.unwrap_err();
        let text = err.to_string();
        assert!(matches!(err, ProviderError::Network(_)), "{text}");
        assert!(!text.contains("SECRET"), "error must not leak the key: {text}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Hits the real API with this machine's own `cmd login` key.
    /// Run with: cargo test --lib live_command_code -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_command_code() {
        let p = CommandCodeProvider::new(reqwest::Client::new());
        let s = p.get_usage().await.expect("live fetch");
        println!("{:#?}", s);
        assert_eq!(s.provider_id, ProviderId::CommandCode);
        assert!(s.plan_name.is_some());
    }
}
