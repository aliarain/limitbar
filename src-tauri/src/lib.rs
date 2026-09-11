mod commands;
mod config;
mod platform;
mod providers;
mod usage;

use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, LogicalPosition, Manager, PhysicalPosition, Position, WindowEvent};
use usage::manager::UsageManager;

/// Event name the widget listens to; payload is `Vec<ProviderView>`.
pub const USAGE_EVENT: &str = "usage://changed";
/// How often the scheduler checks whether any provider is due. Cheap: a lock
/// and a few timestamp comparisons. Actual provider polling is governed by
/// the manager's interval (5 min default), not by this tick.
const SCHEDULER_TICK: Duration = Duration::from_secs(15);
/// Default widget spot: top-centre, just under the menu bar.
const DEFAULT_WIDGET_Y: f64 = 36.0;
const WIDGET_CORNER_RADIUS: f64 = 14.0;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().level(log::LevelFilter::Info).build())
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

            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = Arc::new(config::SettingsStore::load(settings_path));
            app.manage(Arc::clone(&settings));

            let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build()?;
            let manager = Arc::new(UsageManager::new(providers::all(http)));
            app.manage::<commands::ManagerState>(Arc::clone(&manager));

            // State changes fan out to the widget (event) and the tray (menu rebuild).
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

            if let Some(w) = app.get_webview_window("main") {
                platform::glass::apply(&w, WIDGET_CORNER_RADIUS);
                restore_widget_position(&w, &settings.get());
                let store = Arc::clone(&settings);
                let wh = w.clone();
                w.on_window_event(move |event| {
                    if let WindowEvent::Moved(pos) = event {
                        remember_widget_position(&wh, *pos, &store);
                    }
                });
                let _ = w.show();
            }

            spawn_scheduler(Arc::clone(&manager));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running LimitBar");
}

fn restore_widget_position(w: &tauri::WebviewWindow, settings: &config::Settings) {
    let target = match settings.widget_position {
        Some((x, y)) => LogicalPosition::new(x, y),
        None => match w.primary_monitor() {
            Ok(Some(m)) => {
                let scale = m.scale_factor();
                let size = m.size().to_logical::<f64>(scale);
                let origin = m.position().to_logical::<f64>(scale);
                let width = w.outer_size().map(|s| s.to_logical::<f64>(scale).width).unwrap_or(168.0);
                LogicalPosition::new(origin.x + (size.width - width) / 2.0, origin.y + DEFAULT_WIDGET_Y)
            }
            _ => LogicalPosition::new(600.0, DEFAULT_WIDGET_Y),
        },
    };
    if let Err(e) = w.set_position(Position::Logical(target)) {
        log::warn!("cannot position widget: {e}");
    }
}

fn remember_widget_position(w: &tauri::WebviewWindow, pos: PhysicalPosition<i32>, store: &config::SettingsStore) {
    let scale = w.scale_factor().unwrap_or(1.0);
    let logical = pos.to_logical::<f64>(scale);
    store.update(|s| s.widget_position = Some((logical.x, logical.y)));
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
