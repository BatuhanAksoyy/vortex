//! The browser behind every page in the app. It runs while a page is open.

use std::{
    path::Path,
    sync::{Arc, Weak},
};

use anyhow::Context as _;
use gpui_kit::{App, Global};
use kiln_browser::{Chrome, Profile};

#[derive(Default)]
struct Host {
    running: Weak<Chrome>,
    /// Claimed for as long as the app runs, so it stays the same one.
    profile: Option<Profile>,
    /// Whether the user was told the profile is not the usual one.
    told: bool,
}

impl Global for Host {}

/// The running browser, started if there is none.
pub fn chrome(profile: &Path, scale: f32, cx: &mut App) -> anyhow::Result<Arc<Chrome>> {
    let host = cx.default_global::<Host>();
    if let Some(chrome) = host.running.upgrade().filter(|chrome| !chrome.ended()) {
        return Ok(chrome);
    }
    let installed = kiln_browser::find().context(
        "No Chromium-based browser is installed. Install Chrome, Edge, Brave, Chromium or Vivaldi to see pages here.",
    )?;
    let claimed = match host.profile.take() {
        Some(claimed) => claimed,
        None => Profile::claim(profile)?,
    };
    let claimed = host.profile.insert(claimed);
    let chrome = Arc::new(Chrome::launch(&installed, claimed, scale)?);
    host.running = Arc::downgrade(&chrome);
    Ok(chrome)
}

/// Whether the user should be told that another Kiln has the usual profile;
/// true once.
pub fn note_profile(cx: &mut App) -> bool {
    let host = cx.default_global::<Host>();
    let other = host.profile.as_ref().is_some_and(|it| !it.is_first());
    let note = other && !host.told;
    host.told |= note;
    note
}
