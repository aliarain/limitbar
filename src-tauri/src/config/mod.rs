//! Non-sensitive persisted settings (window position, preferences).
//! Stored as JSON in the OS app-config directory. Never holds credentials.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Logical top-left of the widget window, saved when the user drags it.
    pub widget_position: Option<(f64, f64)>,
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Self {
        let current = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).map_err(|e| log::warn!("settings unreadable, using defaults: {e}")).ok())
            .unwrap_or_default();
        Self { path, current: Mutex::new(current) }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().expect("settings lock").clone()
    }

    /// Applies `f` and writes to disk only if something changed.
    pub fn update(&self, f: impl FnOnce(&mut Settings)) {
        let snapshot = {
            let mut cur = self.current.lock().expect("settings lock");
            let before = cur.clone();
            f(&mut cur);
            if *cur == before {
                return;
            }
            cur.clone()
        };
        if let Some(dir) = self.path.parent() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                log::warn!("cannot create settings dir: {e}");
                return;
            }
        }
        match serde_json::to_vec_pretty(&snapshot) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&self.path, bytes) {
                    log::warn!("cannot write settings: {e}");
                }
            }
            Err(e) => log::warn!("cannot serialize settings: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("limitbar-settings-{}-{name}", std::process::id())).join("settings.json")
    }

    #[test]
    fn missing_file_yields_defaults() {
        let s = SettingsStore::load(temp_path("missing"));
        assert_eq!(s.get(), Settings::default());
    }

    #[test]
    fn update_persists_and_reloads() {
        let p = temp_path("roundtrip");
        let s = SettingsStore::load(p.clone());
        s.update(|c| c.widget_position = Some((12.5, 40.0)));
        let again = SettingsStore::load(p.clone());
        assert_eq!(again.get().widget_position, Some((12.5, 40.0)));
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let p = temp_path("corrupt");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"{not json").unwrap();
        let s = SettingsStore::load(p.clone());
        assert_eq!(s.get(), Settings::default());
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let v: Settings = serde_json::from_str(r#"{"widget_position":[1,2],"future_field":true}"#).unwrap();
        assert_eq!(v.widget_position, Some((1.0, 2.0)));
    }
}
