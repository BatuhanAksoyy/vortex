//! The browser behind every page in the app. It runs while a page is open.

use std::{
    path::Path,
    sync::{Arc, Weak},
};

use anyhow::Context as _;
use gpui_kit::{App, Global};
use kiln_browser::Chrome;

#[derive(Default)]
struct Host(Weak<Chrome>);

impl Global for Host {}

/// The running browser, started if there is none.
pub fn chrome(profile: &Path, scale: f32, cx: &mut App) -> anyhow::Result<Arc<Chrome>> {
    let running = cx.default_global::<Host>().0.upgrade();
    if let Some(chrome) = running.filter(|chrome| !chrome.ended()) {
        return Ok(chrome);
    }
    let installed = kiln_browser::find().context(
        "No Chromium-based browser is installed. Install Chrome, Edge, Brave, Chromium or Vivaldi to see pages here.",
    )?;
    let chrome = Arc::new(Chrome::launch(&installed, profile, scale)?);
    cx.set_global(Host(Arc::downgrade(&chrome)));
    Ok(chrome)
}
