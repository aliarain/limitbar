//! Menu bar / system tray. Reads state from the UsageManager via the change
//! listener; never performs network work itself.

use crate::usage::manager::{Freshness, ProviderView};
use crate::usage::models::UsageStatus;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Position, Runtime, WebviewWindow};

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
            MENU_OPEN => show_popup(app, None),
            MENU_QUIT => app.exit(0),
            id if id.starts_with(MENU_PROVIDER_PREFIX) => show_popup(app, None),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                        return;
                    }
                }
                let anchor = rect.position.to_logical::<f64>(1.0);
                let size = rect.size.to_logical::<f64>(1.0);
                show_popup(app, Some((anchor, size)));
            }
        })
        .build(app)?;
    Ok(())
}

/// Rebuilds the native menu from the latest views. Cheap: a handful of items.
pub fn update<R: Runtime>(app: &AppHandle<R>, views: &[ProviderView]) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
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
    b = b.item(&MenuItemBuilder::with_id(MENU_OPEN, "Open LimitBar").build(app)?);
    b = b.item(&PredefinedMenuItem::separator(app)?);
    b = b.item(&MenuItemBuilder::with_id(MENU_QUIT, "Quit LimitBar").build(app)?);
    b.build()
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

/// Shows the popup, positioned under the tray icon when an anchor is known.
fn show_popup<R: Runtime>(app: &AppHandle<R>, anchor: Option<(LogicalPosition<f64>, LogicalSize<f64>)>) {
    let Some(w) = app.get_webview_window("main") else { return };
    if let Some((pos, size)) = anchor {
        position_under_anchor(&w, pos, size);
    }
    let _ = w.show();
    let _ = w.set_focus();
}

fn position_under_anchor<R: Runtime>(w: &WebviewWindow<R>, pos: LogicalPosition<f64>, size: LogicalSize<f64>) {
    let scale = w.scale_factor().unwrap_or(1.0);
    let win = w
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale))
        .unwrap_or(LogicalSize::new(320.0, 340.0));
    let mut x = pos.x + size.width / 2.0 - win.width / 2.0;
    let y = pos.y + size.height + 6.0;
    // Keep inside the monitor that contains the anchor.
    if let Ok(Some(mon)) = w.current_monitor() {
        let mpos = mon.position().to_logical::<f64>(scale);
        let msize = mon.size().to_logical::<f64>(scale);
        let min_x = mpos.x + 8.0;
        let max_x = mpos.x + msize.width - win.width - 8.0;
        x = x.clamp(min_x, max_x.max(min_x));
    }
    let _ = w.set_position(Position::Logical(LogicalPosition::new(x, y)));
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
