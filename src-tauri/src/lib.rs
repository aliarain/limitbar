mod commands;
mod platform;
mod providers;
mod usage;

use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager, WindowEvent};
use usage::manager::UsageManager;

/// Event name the popup listens to; payload is `Vec<ProviderView>`.
pub const USAGE_EVENT: &str = "usage://changed";
/// How often the scheduler checks whether any provider is due. Cheap: a lock
/// and a few timestamp comparisons. Actual provider polling is governed by
/// the manager's interval (5 min default), not by this tick.
const SCHEDULER_TICK: Duration = Duration::from_secs(15);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_usage,
            commands::refresh_usage,
            commands::refresh_provider,
            commands::hide_popup,
            commands::quit_app,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()?;
            let manager = Arc::new(UsageManager::new(providers::all(http)));
            app.manage::<commands::ManagerState>(Arc::clone(&manager));

            // State changes fan out to the popup (event) and the tray (menu rebuild).
            let handle = app.handle().clone();
            let listener: usage::manager::ChangeListener = Arc::new(move |views| {
                if let Err(e) = handle.emit(USAGE_EVENT, views) {
                    log::warn!("emit failed: {e}");
                }
                platform::tray::update(&handle, views);
            });
            tauri::async_runtime::block_on(manager.set_listener(listener));

            let initial = tauri::async_runtime::block_on(manager.views());
            platform::tray::build(app.handle(), &initial)?;
            log::info!("tray icon registered");

            // Hide the popup when it loses focus, like a native menu-bar popover.
            // (Kept open under the dev flag so it can be inspected.)
            let pin_open = std::env::var_os("LIMITBAR_SHOW_ON_START").is_some();
            if let Some(w) = app.get_webview_window("main") {
                let wh = w.clone();
                w.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        if !pin_open {
                            let _ = wh.hide();
                        }
                    }
                });
            }

            spawn_scheduler(Arc::clone(&manager));

            // Development aid: `LIMITBAR_SHOW_ON_START=1` opens the popup immediately
            // (useful on notched displays where a new menu-bar item can be hidden).
            if pin_open {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Wait for the run loop; showing inside setup races app activation.
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    if let Some(w) = handle.get_webview_window("main") {
                        let _ = w.center();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running LimitBar");
}

/// Wakes every SCHEDULER_TICK, refreshes only providers whose next_due_at has
/// passed. No busy loop; idle cost is one timer wake-up per tick.
fn spawn_scheduler(manager: Arc<UsageManager>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(SCHEDULER_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let due = manager.due(chrono::Utc::now()).await;
            if !due.is_empty() {
                manager.refresh_many(due).await;
            }
        }
    });
}
