//! IPC surface for the popup window. Every command validates its inputs and
//! returns only non-sensitive data.

use crate::usage::manager::{ProviderView, UsageManager};
use crate::usage::models::ProviderId;
use std::sync::Arc;
use tauri::State;

pub type ManagerState = Arc<UsageManager>;

#[tauri::command]
pub async fn get_usage(manager: State<'_, ManagerState>) -> Result<Vec<ProviderView>, String> {
    Ok(manager.views().await)
}

#[tauri::command]
pub async fn refresh_usage(manager: State<'_, ManagerState>) -> Result<Vec<ProviderView>, String> {
    let m = Arc::clone(&manager);
    m.refresh_all().await;
    Ok(m.views().await)
}

#[tauri::command]
pub async fn refresh_provider(
    provider_id: ProviderId,
    manager: State<'_, ManagerState>,
) -> Result<Vec<ProviderView>, String> {
    manager.refresh(provider_id).await;
    Ok(manager.views().await)
}

#[tauri::command]
pub async fn hide_popup(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}
