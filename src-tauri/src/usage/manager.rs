//! Owns per-provider state: last good snapshot, failure bookkeeping, backoff,
//! and the "is it due" decision. Tauri-agnostic; the app layer wires a change
//! callback to window events and the tray.

use super::models::{ProviderError, ProviderId, UsageSnapshot, UsageStatus};
use crate::providers::UsageProvider;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub const DEFAULT_INTERVAL_SECS: u64 = 300;
const BACKOFF_BASE_SECS: i64 = 30;
const MAX_RATE_LIMIT_WAIT_SECS: i64 = 3600;
/// Grace after a reset instant before re-fetching, so the provider has flipped.
const RESET_GRACE_SECS: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Freshness {
    /// Never fetched successfully and not currently fetching.
    Never,
    Fresh,
    Refreshing,
    Stale,
}

/// What the UI and tray see for one provider. Never contains credentials.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    pub provider_id: ProviderId,
    pub provider_name: String,
    pub enabled: bool,
    /// Last *successful* snapshot, kept through later failures.
    pub snapshot: Option<UsageSnapshot>,
    /// Effective status: the last attempt's failure if it failed, else the snapshot's.
    pub status: UsageStatus,
    pub freshness: Freshness,
    pub last_error: Option<String>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub next_due_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
struct ProviderState {
    enabled: bool,
    snapshot: Option<UsageSnapshot>,
    last_attempt_at: Option<DateTime<Utc>>,
    /// Set when the most recent attempt failed; cleared on success.
    last_failure: Option<(UsageStatus, String)>,
    consecutive_failures: u32,
    refreshing: bool,
    next_due_at: Option<DateTime<Utc>>,
}

pub type ChangeListener = Arc<dyn Fn(&[ProviderView]) + Send + Sync>;

pub struct UsageManager {
    providers: Vec<Arc<dyn UsageProvider>>,
    states: RwLock<HashMap<ProviderId, ProviderState>>,
    interval_secs: AtomicU64,
    listener: RwLock<Option<ChangeListener>>,
}

impl UsageManager {
    pub fn new(providers: Vec<Box<dyn UsageProvider>>) -> Self {
        let providers: Vec<Arc<dyn UsageProvider>> = providers.into_iter().map(Arc::from).collect();
        let states = providers
            .iter()
            .map(|p| {
                (
                    p.id(),
                    ProviderState { enabled: true, next_due_at: Some(Utc::now()), ..Default::default() },
                )
            })
            .collect();
        Self {
            providers,
            states: RwLock::new(states),
            interval_secs: AtomicU64::new(DEFAULT_INTERVAL_SECS),
            listener: RwLock::new(None),
        }
    }

    pub async fn set_listener(&self, l: ChangeListener) {
        *self.listener.write().await = Some(l);
    }

    pub fn interval(&self) -> ChronoDuration {
        ChronoDuration::seconds(self.interval_secs.load(Ordering::Relaxed) as i64)
    }

    pub async fn views(&self) -> Vec<ProviderView> {
        let states = self.states.read().await;
        let now = Utc::now();
        self.providers
            .iter()
            .map(|p| {
                let s = states.get(&p.id()).expect("state for every provider");
                self.view_of(p.as_ref(), s, now)
            })
            .collect()
    }

    fn view_of(&self, p: &dyn UsageProvider, s: &ProviderState, now: DateTime<Utc>) -> ProviderView {
        let status = match (&s.last_failure, &s.snapshot) {
            (Some((st, _)), _) => *st,
            (None, Some(snap)) => snap.status,
            (None, None) => UsageStatus::Unavailable,
        };
        let freshness = if s.refreshing {
            Freshness::Refreshing
        } else if let Some(snap) = &s.snapshot {
            let age = now - snap.fetched_at;
            if s.last_failure.is_some() || age > self.interval() * 2 {
                Freshness::Stale
            } else {
                Freshness::Fresh
            }
        } else {
            Freshness::Never
        };
        ProviderView {
            provider_id: p.id(),
            provider_name: p.name().to_string(),
            enabled: s.enabled,
            snapshot: s.snapshot.clone(),
            status,
            freshness,
            last_error: s.last_failure.as_ref().map(|(_, m)| m.clone()),
            last_attempt_at: s.last_attempt_at,
            next_due_at: s.next_due_at,
        }
    }

    async fn notify(&self) {
        let views = self.views().await;
        if let Some(l) = self.listener.read().await.as_ref() {
            l(&views);
        }
    }

    /// Providers whose next_due_at has passed and are not already in flight.
    pub async fn due(&self, now: DateTime<Utc>) -> Vec<ProviderId> {
        let states = self.states.read().await;
        self.providers
            .iter()
            .map(|p| p.id())
            .filter(|id| {
                let s = &states[id];
                s.enabled && !s.refreshing && s.next_due_at.map_or(false, |t| t <= now)
            })
            .collect()
    }

    /// Refreshes every enabled provider concurrently. Used for manual refresh.
    pub async fn refresh_all(self: &Arc<Self>) {
        let ids: Vec<ProviderId> = self.providers.iter().map(|p| p.id()).collect();
        self.refresh_many(ids).await;
    }

    pub async fn refresh_many(self: &Arc<Self>, ids: Vec<ProviderId>) {
        let handles: Vec<_> = ids
            .into_iter()
            .map(|id| {
                let me = Arc::clone(self);
                tokio::spawn(async move { me.refresh(id).await })
            })
            .collect();
        for h in handles {
            let _ = h.await;
        }
    }

    /// Fetches one provider. Returns immediately if disabled or already in flight.
    pub async fn refresh(&self, id: ProviderId) {
        let Some(provider) = self.providers.iter().find(|p| p.id() == id).cloned() else {
            return;
        };
        {
            let mut states = self.states.write().await;
            let s = states.get_mut(&id).expect("state");
            if !s.enabled || s.refreshing {
                return;
            }
            s.refreshing = true;
        }
        self.notify().await;

        let result = provider.get_usage().await;
        let now = Utc::now();

        {
            let mut states = self.states.write().await;
            let s = states.get_mut(&id).expect("state");
            s.refreshing = false;
            s.last_attempt_at = Some(now);
            match result {
                Ok(snapshot) => {
                    s.consecutive_failures = 0;
                    s.last_failure = None;
                    s.next_due_at = Some(next_due_after_success(&snapshot, now, self.interval()));
                    s.snapshot = Some(snapshot);
                    log::info!("{id}: refreshed");
                }
                Err(err) => {
                    s.consecutive_failures = s.consecutive_failures.saturating_add(1);
                    let (status, wait) = classify_failure(&err, s.consecutive_failures, self.interval());
                    s.last_failure = Some((status, err.to_string()));
                    s.next_due_at = Some(now + wait);
                    log::warn!("{id}: refresh failed ({err}); retry in {}s", wait.num_seconds());
                }
            }
        }
        self.notify().await;
    }
}

fn next_due_after_success(
    snapshot: &UsageSnapshot,
    now: DateTime<Utc>,
    interval: ChronoDuration,
) -> DateTime<Utc> {
    let regular = now + interval;
    match snapshot.next_reset_at() {
        Some(reset) if reset > now => regular.min(reset + ChronoDuration::seconds(RESET_GRACE_SECS)),
        _ => regular,
    }
}

fn classify_failure(
    err: &ProviderError,
    consecutive_failures: u32,
    interval: ChronoDuration,
) -> (UsageStatus, ChronoDuration) {
    match err {
        // A missing login rarely fixes itself within seconds; poll at the normal cadence.
        ProviderError::AuthRequired => (UsageStatus::AuthRequired, interval),
        ProviderError::Unsupported(_) => (UsageStatus::Unsupported, interval),
        ProviderError::RateLimited { retry_after_secs } => {
            let secs = retry_after_secs
                .map(|s| s as i64)
                .unwrap_or_else(|| interval.num_seconds())
                .clamp(BACKOFF_BASE_SECS, MAX_RATE_LIMIT_WAIT_SECS);
            (UsageStatus::RateLimited, ChronoDuration::seconds(secs))
        }
        ProviderError::Network(_) | ProviderError::Http { .. } | ProviderError::Parse(_) => {
            let exp = consecutive_failures.saturating_sub(1).min(10);
            let secs = (BACKOFF_BASE_SECS << exp).min(interval.num_seconds());
            (UsageStatus::Error, ChronoDuration::seconds(secs))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::models::{UsageSource, UsageWindow};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct Mock {
        script: Mutex<VecDeque<Result<UsageSnapshot, ProviderError>>>,
        calls: std::sync::atomic::AtomicU32,
    }

    #[async_trait]
    impl UsageProvider for Mock {
        fn id(&self) -> ProviderId { ProviderId::CommandCode }
        fn name(&self) -> &'static str { "Mock" }
        async fn get_usage(&self) -> Result<UsageSnapshot, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.script.lock().unwrap().pop_front().unwrap_or(Err(ProviderError::Network("script exhausted".into())))
        }
    }

    fn snapshot(remaining: f64, reset_in: Option<ChronoDuration>) -> UsageSnapshot {
        let mut w = UsageWindow::from_used_cap("w", "W", 100.0 - remaining, 100.0);
        w.reset_at = reset_in.map(|d| Utc::now() + d);
        UsageSnapshot {
            provider_id: ProviderId::CommandCode,
            provider_name: "Mock".into(),
            status: UsageStatus::Available,
            source: UsageSource::OfficialApi,
            windows: vec![w],
            plan_name: None,
            account_identifier: None,
            detail: None,
            message: None,
            fetched_at: Utc::now(),
        }
    }

    fn manager(script: Vec<Result<UsageSnapshot, ProviderError>>) -> Arc<UsageManager> {
        let mock = Mock { script: Mutex::new(script.into()), calls: 0.into() };
        Arc::new(UsageManager::new(vec![Box::new(mock)]))
    }

    #[tokio::test]
    async fn initial_state_is_due_and_never_fetched() {
        let m = manager(vec![]);
        assert_eq!(m.due(Utc::now()).await, vec![ProviderId::CommandCode]);
        let v = &m.views().await[0];
        assert_eq!(v.freshness, Freshness::Never);
        assert_eq!(v.status, UsageStatus::Unavailable);
        assert!(v.snapshot.is_none());
    }

    #[tokio::test]
    async fn success_then_failure_keeps_snapshot_and_marks_stale() {
        let m = manager(vec![Ok(snapshot(72.0, None)), Err(ProviderError::Network("boom".into()))]);
        m.refresh(ProviderId::CommandCode).await;
        let v = &m.views().await[0];
        assert_eq!(v.freshness, Freshness::Fresh);
        assert_eq!(v.status, UsageStatus::Available);
        assert_eq!(v.snapshot.as_ref().unwrap().min_remaining_percent(), Some(72.0));

        m.refresh(ProviderId::CommandCode).await;
        let v = &m.views().await[0];
        assert_eq!(v.freshness, Freshness::Stale);
        assert_eq!(v.status, UsageStatus::Error);
        assert_eq!(v.last_error.as_deref(), Some("network error: boom"));
        // Previous data is retained, not zeroed.
        assert_eq!(v.snapshot.as_ref().unwrap().min_remaining_percent(), Some(72.0));
    }

    #[tokio::test]
    async fn auth_required_surfaces_without_dropping_cache() {
        let m = manager(vec![Ok(snapshot(50.0, None)), Err(ProviderError::AuthRequired)]);
        m.refresh(ProviderId::CommandCode).await;
        m.refresh(ProviderId::CommandCode).await;
        let v = &m.views().await[0];
        assert_eq!(v.status, UsageStatus::AuthRequired);
        assert!(v.snapshot.is_some());
        let wait = v.next_due_at.unwrap() - v.last_attempt_at.unwrap();
        assert_eq!(wait.num_seconds(), DEFAULT_INTERVAL_SECS as i64);
    }

    #[tokio::test]
    async fn exponential_backoff_capped_at_interval() {
        let m = manager(vec![
            Err(ProviderError::Network("1".into())),
            Err(ProviderError::Network("2".into())),
            Err(ProviderError::Network("3".into())),
            Err(ProviderError::Network("4".into())),
            Err(ProviderError::Network("5".into())),
        ]);
        let mut waits = Vec::new();
        for _ in 0..5 {
            m.refresh(ProviderId::CommandCode).await;
            let v = &m.views().await[0];
            waits.push((v.next_due_at.unwrap() - v.last_attempt_at.unwrap()).num_seconds());
        }
        assert_eq!(waits, vec![30, 60, 120, 240, 300]);
    }

    #[tokio::test]
    async fn rate_limit_honours_retry_after_within_bounds() {
        let (_, w) = classify_failure(&ProviderError::RateLimited { retry_after_secs: Some(90) }, 1, ChronoDuration::seconds(300));
        assert_eq!(w.num_seconds(), 90);
        let (_, w) = classify_failure(&ProviderError::RateLimited { retry_after_secs: Some(1) }, 1, ChronoDuration::seconds(300));
        assert_eq!(w.num_seconds(), 30);
        let (_, w) = classify_failure(&ProviderError::RateLimited { retry_after_secs: Some(999_999) }, 1, ChronoDuration::seconds(300));
        assert_eq!(w.num_seconds(), 3600);
        let (st, w) = classify_failure(&ProviderError::RateLimited { retry_after_secs: None }, 1, ChronoDuration::seconds(300));
        assert_eq!(st, UsageStatus::RateLimited);
        assert_eq!(w.num_seconds(), 300);
    }

    #[tokio::test]
    async fn refetches_shortly_after_reset_when_sooner_than_interval() {
        let m = manager(vec![Ok(snapshot(10.0, Some(ChronoDuration::seconds(40))))]);
        m.refresh(ProviderId::CommandCode).await;
        let v = &m.views().await[0];
        let wait = (v.next_due_at.unwrap() - v.last_attempt_at.unwrap()).num_seconds();
        assert!((44..=46).contains(&wait), "expected reset+grace, got {wait}s");
    }

    #[tokio::test]
    async fn past_reset_does_not_cause_tight_loop() {
        let m = manager(vec![Ok(snapshot(10.0, Some(ChronoDuration::seconds(-40))))]);
        m.refresh(ProviderId::CommandCode).await;
        let v = &m.views().await[0];
        let wait = (v.next_due_at.unwrap() - v.last_attempt_at.unwrap()).num_seconds();
        assert_eq!(wait, DEFAULT_INTERVAL_SECS as i64);
    }

    #[tokio::test]
    async fn stale_after_two_intervals_even_without_failure() {
        let m = manager(vec![Ok(snapshot(10.0, None))]);
        m.interval_secs.store(60, Ordering::Relaxed);
        m.refresh(ProviderId::CommandCode).await;
        {
            let mut states = m.states.write().await;
            let s = states.get_mut(&ProviderId::CommandCode).unwrap();
            s.snapshot.as_mut().unwrap().fetched_at = Utc::now() - ChronoDuration::seconds(121);
        }
        assert_eq!(m.views().await[0].freshness, Freshness::Stale);
    }

    #[tokio::test]
    async fn concurrent_refresh_is_deduplicated() {
        struct Slow;
        #[async_trait]
        impl UsageProvider for Slow {
            fn id(&self) -> ProviderId { ProviderId::CommandCode }
            fn name(&self) -> &'static str { "Slow" }
            async fn get_usage(&self) -> Result<UsageSnapshot, ProviderError> {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                CALLS.fetch_add(1, Ordering::SeqCst);
                Ok(snapshot(1.0, None))
            }
        }
        static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let m = Arc::new(UsageManager::new(vec![Box::new(Slow)]));
        let a = { let m = m.clone(); tokio::spawn(async move { m.refresh(ProviderId::CommandCode).await }) };
        let b = { let m = m.clone(); tokio::spawn(async move { m.refresh(ProviderId::CommandCode).await }) };
        let _ = tokio::join!(a, b);
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
        assert!(m.due(Utc::now()).await.is_empty());
    }

    #[tokio::test]
    async fn disabled_provider_is_never_fetched() {
        let m = manager(vec![Ok(snapshot(1.0, None))]);
        m.states.write().await.get_mut(&ProviderId::CommandCode).unwrap().enabled = false;
        assert!(m.due(Utc::now()).await.is_empty());
        m.refresh(ProviderId::CommandCode).await;
        assert!(m.views().await[0].snapshot.is_none());
    }

    #[tokio::test]
    async fn listener_is_notified_on_change() {
        let m = manager(vec![Ok(snapshot(1.0, None))]);
        let count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        m.set_listener(Arc::new(move |_| { c.fetch_add(1, Ordering::SeqCst); })).await;
        m.refresh(ProviderId::CommandCode).await;
        // once for "refreshing", once for the result
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }
}
