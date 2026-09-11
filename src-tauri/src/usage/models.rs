//! Provider-agnostic usage data model shared by the backend and the UI.
//!
//! Every field that carries a number the user might act on is `Option` and
//! defaults to `None`: absent data is rendered as "unavailable", never as 0.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable provider identifier. Never a display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    Claude,
    Codex,
    CommandCode,
    Gemini,
}

impl ProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex",
            ProviderId::CommandCode => "command-code",
            ProviderId::Gemini => "gemini",
        }
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UsageStatus {
    Available,
    Unavailable,
    AuthRequired,
    RateLimited,
    Error,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UsageSource {
    OfficialApi,
    LocalProviderData,
    LocalCli,
    /// Never authoritative. The UI must label it and must not treat it as a quota.
    Estimated,
}

/// One rate-limit window (e.g. "5-hour", "weekly"). Providers may expose several.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageWindow {
    /// Stable key within the provider, e.g. `five_hour`.
    pub id: String,
    /// Short human label, e.g. "5h" or "Week".
    pub label: String,
    /// 0..=100. `None` when the provider gave no trustworthy number.
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    /// Absolute instant the window resets. Rendered locally by the UI.
    pub reset_at: Option<DateTime<Utc>>,
    /// Optional provider-supplied wording when no timestamp exists.
    pub reset_description: Option<String>,
    /// The provider has told us this window is currently exhausted.
    pub exceeded: bool,
    /// A cap on one model/feature rather than the whole account. Shown in the
    /// detail view but excluded from the headline percentage.
    pub scoped: bool,
}

impl UsageWindow {
    /// Builds a window from used/cap, clamping to 0..=100 and refusing
    /// non-finite or non-positive caps.
    pub fn from_used_cap(id: &str, label: &str, used: f64, cap: f64) -> Self {
        let pct = if cap.is_finite() && cap > 0.0 && used.is_finite() {
            Some((used / cap * 100.0).clamp(0.0, 100.0))
        } else {
            None
        };
        Self {
            id: id.to_string(),
            label: label.to_string(),
            used_percent: pct,
            remaining_percent: pct.map(|p| 100.0 - p),
            reset_at: None,
            reset_description: None,
            exceeded: false,
            scoped: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub provider_id: ProviderId,
    pub provider_name: String,
    pub status: UsageStatus,
    pub source: UsageSource,
    /// Windows in display priority order; `windows[0]` is the primary meter.
    pub windows: Vec<UsageWindow>,
    pub plan_name: Option<String>,
    pub account_identifier: Option<String>,
    /// Free-form, non-sensitive extra line for the detail view
    /// (e.g. "67.7 credits left · period ends Oct 5").
    pub detail: Option<String>,
    /// Human-readable reason when `status != Available`. Never contains secrets.
    pub message: Option<String>,
    pub fetched_at: DateTime<Utc>,
}

impl UsageSnapshot {
    /// Earliest upcoming reset across all windows, if any.
    pub fn next_reset_at(&self) -> Option<DateTime<Utc>> {
        self.windows.iter().filter_map(|w| w.reset_at).min()
    }

    /// Lowest remaining percentage across account-level windows — the headline.
    /// Model-scoped caps are excluded so one exhausted model does not read as 0%.
    pub fn min_remaining_percent(&self) -> Option<f64> {
        self.windows
            .iter()
            .filter(|w| !w.scoped)
            .filter_map(|w| w.remaining_percent)
            .fold(None, |acc, p| Some(acc.map_or(p, |a: f64| a.min(p))))
    }
}

/// Failure modes a provider can report. The manager maps these to
/// `UsageStatus` and decides whether to keep the previous snapshot.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("not signed in")]
    AuthRequired,
    #[error("rate limited by provider")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("network error: {0}")]
    Network(String),
    #[error("provider returned {status}")]
    Http { status: u16 },
    #[error("unexpected response: {0}")]
    Parse(String),
    #[error("{0}")]
    Unsupported(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_used_cap_computes_and_clamps() {
        let w = UsageWindow::from_used_cap("w", "W", 2.334, 35.0);
        assert!((w.used_percent.unwrap() - 6.668).abs() < 0.01);
        assert!((w.remaining_percent.unwrap() - 93.332).abs() < 0.01);

        let over = UsageWindow::from_used_cap("w", "W", 50.0, 35.0);
        assert_eq!(over.used_percent, Some(100.0));
        assert_eq!(over.remaining_percent, Some(0.0));

        let neg = UsageWindow::from_used_cap("w", "W", -1.0, 35.0);
        assert_eq!(neg.used_percent, Some(0.0));
    }

    #[test]
    fn from_used_cap_refuses_bad_caps() {
        assert_eq!(UsageWindow::from_used_cap("w", "W", 1.0, 0.0).used_percent, None);
        assert_eq!(UsageWindow::from_used_cap("w", "W", 1.0, -5.0).used_percent, None);
        assert_eq!(UsageWindow::from_used_cap("w", "W", 1.0, f64::NAN).used_percent, None);
        assert_eq!(UsageWindow::from_used_cap("w", "W", f64::INFINITY, 5.0).used_percent, None);
    }

    #[test]
    fn provider_id_serializes_as_kebab_case() {
        assert_eq!(serde_json::to_string(&ProviderId::CommandCode).unwrap(), "\"command-code\"");
        assert_eq!(ProviderId::CommandCode.as_str(), "command-code");
    }

    fn snap(windows: Vec<UsageWindow>) -> UsageSnapshot {
        UsageSnapshot {
            provider_id: ProviderId::CommandCode,
            provider_name: "Command Code".into(),
            status: UsageStatus::Available,
            source: UsageSource::OfficialApi,
            windows,
            plan_name: None,
            account_identifier: None,
            detail: None,
            message: None,
            fetched_at: Utc::now(),
        }
    }

    #[test]
    fn aggregates_across_windows() {
        let t1 = Utc::now() + chrono::Duration::hours(3);
        let t2 = Utc::now() + chrono::Duration::hours(1);
        let mut a = UsageWindow::from_used_cap("a", "A", 10.0, 100.0);
        a.reset_at = Some(t1);
        let mut b = UsageWindow::from_used_cap("b", "B", 60.0, 100.0);
        b.reset_at = Some(t2);
        let s = snap(vec![a, b]);
        assert_eq!(s.next_reset_at(), Some(t2));
        assert_eq!(s.min_remaining_percent(), Some(40.0));
    }

    #[test]
    fn scoped_windows_do_not_drive_the_headline() {
        let account = UsageWindow::from_used_cap("a", "A", 40.0, 100.0);
        let mut model = UsageWindow::from_used_cap("m", "M", 100.0, 100.0);
        model.scoped = true;
        let s = snap(vec![account, model.clone()]);
        assert_eq!(s.min_remaining_percent(), Some(60.0));
        assert_eq!(snap(vec![model]).min_remaining_percent(), None);
    }

    #[test]
    fn aggregates_are_none_when_no_data() {
        let s = snap(vec![]);
        assert_eq!(s.next_reset_at(), None);
        assert_eq!(s.min_remaining_percent(), None);
    }
}
