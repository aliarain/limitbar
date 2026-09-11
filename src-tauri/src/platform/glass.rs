//! macOS Liquid Glass background (macOS 26+). Inserts an `NSGlassEffectView`
//! beneath the webview so the transparent page renders on real system glass.
//! Older systems fall back to Tauri's vibrancy material.

#[cfg(target_os = "macos")]
pub fn apply(window: &tauri::WebviewWindow, corner_radius: f64) {
    use objc2::rc::Retained;
    use objc2::runtime::AnyClass;
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSGlassEffectView, NSView, NSWindow, NSWindowOrderingMode};

    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("glass: not on main thread");
        return;
    };
    if AnyClass::get(c"NSGlassEffectView").is_none() {
        log::info!("glass: NSGlassEffectView unavailable, using vibrancy");
        fallback(window, corner_radius);
        return;
    }
    let Ok(ptr) = window.ns_window() else {
        log::warn!("glass: no NSWindow");
        return;
    };
    // SAFETY: Tauri hands out the live NSWindow pointer for this window; we only
    // touch it on the main thread while the window is alive.
    let ns_window: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
    let Some(content) = ns_window.contentView() else { return };

    let glass: Retained<NSGlassEffectView> =
        NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), content.bounds());
    glass.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
    glass.setCornerRadius(corner_radius);
    let glass_view: &NSView = &glass;
    content.addSubview_positioned_relativeTo(glass_view, NSWindowOrderingMode::Below, None);
    log::info!("glass: NSGlassEffectView attached");
}

#[cfg(target_os = "macos")]
fn fallback(window: &tauri::WebviewWindow, corner_radius: f64) {
    use tauri::window::{Effect, EffectsBuilder};
    let effects = EffectsBuilder::new().effect(Effect::Popover).radius(corner_radius).build();
    if let Err(e) = window.set_effects(effects) {
        log::warn!("glass: vibrancy fallback failed: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply(_window: &tauri::WebviewWindow, _corner_radius: f64) {}
