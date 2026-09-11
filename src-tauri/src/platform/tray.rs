//! Menu bar / system tray. Reads state from the UsageManager via the change
//! listener; never performs network work itself.

use crate::usage::manager::{Freshness, ProviderView};
use crate::usage::models::UsageStatus;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

pub const TRAY_ID: &str = "limitbar-tray";
const MENU_REFRESH: &str = "refresh";
const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";
const MENU_PROVIDER_PREFIX: &str = "provider:";

pub fn build<R: Runtime>(app: &AppHandle<R>, views: &[ProviderView]) -> tauri::Result<()> {
    let icon = tauri::image::Image::from_bytes(include_bytes!("../../icons/tray-icon@2x.png"))?;
    let menu = build_menu(app, views)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("LimitBar")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_REFRESH => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let m = app.state::<crate::commands::ManagerState>().inner().clone();
                    m.refresh_all().await;
                });
            }
            MENU_OPEN => show_widget(app),
            MENU_QUIT => app.exit(0),
            id if id.starts_with(MENU_PROVIDER_PREFIX) => show_widget(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_widget(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Rebuilds the native menu from the latest views. Cheap: a handful of items.
pub fn update<R: Runtime>(app: &AppHandle<R>, views: &[ProviderView]) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    #[cfg(target_os = "macos")]
    if let Err(e) = tray.set_title(tray_title(views)) {
        log::warn!("tray title update failed: {e}");
    }
    match build_menu(app, views) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                log::warn!("tray menu update failed: {e}");
            }
        }
        Err(e) => log::warn!("tray menu build failed: {e}"),
    }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, views: &[ProviderView]) -> tauri::Result<Menu<R>> {
    let mut b = MenuBuilder::new(app);
    b = b.item(&MenuItemBuilder::with_id("title", "LimitBar").enabled(false).build(app)?);
    b = b.item(&PredefinedMenuItem::separator(app)?);
    for v in views.iter().filter(|v| v.enabled) {
        let id = format!("{MENU_PROVIDER_PREFIX}{}", v.provider_id);
        b = b.item(&MenuItemBuilder::with_id(id, menu_line(v)).build(app)?);
    }
    b = b.item(&PredefinedMenuItem::separator(app)?);
    b = b.item(&MenuItemBuilder::with_id(MENU_REFRESH, "Refresh").build(app)?);
    b = b.item(&MenuItemBuilder::with_id(MENU_OPEN, "Show Widget").build(app)?);
    b = b.item(&PredefinedMenuItem::separator(app)?);
    b = b.item(&MenuItemBuilder::with_id(MENU_QUIT, "Quit LimitBar").build(app)?);
    b.build()
}

/// Text next to the menu-bar icon: the lowest remaining % across providers
/// that have a trustworthy number. `None` hides the text entirely.
pub fn tray_title(views: &[ProviderView]) -> Option<String> {
    views
        .iter()
        .filter(|v| v.enabled)
        .filter_map(|v| v.snapshot.as_ref().and_then(|s| s.min_remaining_percent()))
        .fold(None, |acc: Option<f64>, p| Some(acc.map_or(p, |a| a.min(p))))
        .map(|p| format!("{}%", p.round() as i64))
}

/// One line per provider for the native menu, e.g. "Command Code   93%".
pub fn menu_line(v: &ProviderView) -> String {
    let value = match (v.status, v.snapshot.as_ref().and_then(|s| s.min_remaining_percent())) {
        (UsageStatus::AuthRequired, _) => "sign in".to_string(),
        (UsageStatus::RateLimited, Some(p)) | (UsageStatus::Error, Some(p)) => format!("{}% (stale)", p.round() as i64),
        (UsageStatus::Available, Some(p)) => {
            if v.freshness == Freshness::Stale { format!("{}% (stale)", p.round() as i64) } else { format!("{}%", p.round() as i64) }
        }
        (UsageStatus::Error, None) => "error".to_string(),
        _ if v.freshness == Freshness::Refreshing || v.freshness == Freshness::Never => "…".to_string(),
        _ => "unavailable".to_string(),
    };
    format!("{}\t{}", v.provider_name, value)
}

fn show_widget<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
    }
}

fn toggle_widget<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window("main") else { return };
    if w.is_visible().unwrap_or(false) {
        log::info!("widget hidden (tray click)");
        let _ = w.hide();
    } else {
        log::info!("widget shown (tray click)");
        let _ = w.show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::models::{ProviderId, UsageSnapshot, UsageSource, UsageWindow};
    use chrono::Utc;

    fn view(status: UsageStatus, freshness: Freshness, remaining: Option<f64>) -> ProviderView {
        let snapshot = remaining.map(|r| UsageSnapshot {
            provider_id: ProviderId::CommandCode,
            provider_name: "Command Code".into(),
            status: UsageStatus::Available,
            source: UsageSource::OfficialApi,
            windows: vec![UsageWindow::from_used_cap("w", "W", 100.0 - r, 100.0)],
            plan_name: None,
            account_identifier: None,
            detail: None,
            message: None,
            fetched_at: Utc::now(),
        });
        ProviderView {
            provider_id: ProviderId::CommandCode,
            provider_name: "Command Code".into(),
            enabled: true,
            snapshot,
            status,
            freshness,
            last_error: None,
            last_attempt_at: None,
            next_due_at: None,
        }
    }

    #[test]
    fn tray_title_is_min_across_providers_or_none() {
        assert_eq!(tray_title(&[view(UsageStatus::Available, Freshness::Fresh, Some(93.3)), view(UsageStatus::Error, Freshness::Stale, Some(40.2))]), Some("40%".into()));
        assert_eq!(tray_title(&[view(UsageStatus::AuthRequired, Freshness::Never, None)]), None);
        assert_eq!(tray_title(&[]), None);
        let mut disabled = view(UsageStatus::Available, Freshness::Fresh, Some(10.0));
        disabled.enabled = false;
        assert_eq!(tray_title(&[disabled]), None);
    }

    #[test]
    fn menu_lines_cover_states() {
        assert_eq!(menu_line(&view(UsageStatus::Available, Freshness::Fresh, Some(93.3))), "Command Code\t93%");
        assert_eq!(menu_line(&view(UsageStatus::Available, Freshness::Stale, Some(93.3))), "Command Code\t93% (stale)");
        assert_eq!(menu_line(&view(UsageStatus::Error, Freshness::Stale, Some(40.0))), "Command Code\t40% (stale)");
        assert_eq!(menu_line(&view(UsageStatus::Error, Freshness::Never, None)), "Command Code\terror");
        assert_eq!(menu_line(&view(UsageStatus::AuthRequired, Freshness::Never, None)), "Command Code\tsign in");
        assert_eq!(menu_line(&view(UsageStatus::Unavailable, Freshness::Never, None)), "Command Code\t…");
        assert_eq!(menu_line(&view(UsageStatus::Unavailable, Freshness::Fresh, None)), "Command Code\tunavailable");
    }
}
