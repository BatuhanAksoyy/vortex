//! The running browser.

use std::{
    path::Path,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use async_channel::Receiver;
use serde_json::json;

use crate::{
    event::PageEvent,
    find::Installed,
    page::{Page, Viewport},
    transport::Transport,
};

/// The browser reads commands from descriptor 3 and writes to 4. The shell
/// puts its pipes there and becomes the browser; setting them up in this
/// process instead would take `unsafe`.
const CONNECT: &str = r#"exec 3<&0 4>&1 </dev/null >/dev/null; exec "$@""#;

const ARGS: [&str; 8] = [
    "--headless=new",
    // Not a port, which any program on the machine could connect to.
    "--remote-debugging-pipe",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--disable-sync",
    "--disable-component-update",
    "--disable-background-networking",
];

/// How long the browser has to save its profile when it is closed.
const CLOSE_TIME: Duration = Duration::from_secs(3);

pub struct Chrome {
    child: Option<Child>,
    transport: Arc<Transport>,
    /// Device pixels per page pixel in the frames.
    scale: f32,
}

impl Chrome {
    /// Starts `browser` with the profile in `profile`, which no other
    /// running browser may use.
    pub fn launch(browser: &Installed, profile: &Path, scale: f32) -> anyhow::Result<Self> {
        std::fs::create_dir_all(profile)
            .with_context(|| format!("cannot create {}", profile.display()))?;
        let mut data = std::ffi::OsString::from("--user-data-dir=");
        data.push(profile);
        let mut child = Command::new("/bin/sh")
            .args(["-c", CONNECT, "kiln-browser"])
            .arg(&browser.path)
            .args(ARGS)
            .arg(data)
            // Frames come in page pixels unless the whole browser is scaled.
            .arg(format!("--force-device-scale-factor={scale}"))
            .arg("about:blank")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("cannot start {}", browser.name))?;
        let input = child.stdin.take().context("the browser has no input")?;
        let output = child.stdout.take().context("the browser has no output")?;
        let transport = Transport::start(input, output);
        transport.send(
            "Target.setDiscoverTargets",
            &json!({"discover": true}),
            None,
        );
        // Nothing asks where to save yet.
        transport.send(
            "Browser.setDownloadBehavior",
            &json!({"behavior": "deny"}),
            None,
        );
        Ok(Self {
            child: Some(child),
            transport,
            scale,
        })
    }

    #[must_use]
    pub fn ended(&self) -> bool {
        self.transport.ended()
    }

    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Opens a blank page, whose events end when it or the browser does.
    pub async fn open_page(
        &self,
        viewport: Viewport,
    ) -> anyhow::Result<(Page, Receiver<PageEvent>)> {
        Page::open(self.transport.clone(), viewport).await
    }
}

impl Drop for Chrome {
    /// Asked first, so it saves the profile; its pages' sign-ins are there.
    fn drop(&mut self) {
        self.transport.send("Browser.close", &json!({}), None);
        let Some(mut child) = self.child.take() else {
            return;
        };
        std::thread::spawn(move || {
            if !ends(&mut child) {
                let _ = child.kill();
                let _ = child.wait();
            }
        });
    }
}

/// Whether the process ends by itself in the time it has.
fn ends(child: &mut Child) -> bool {
    let step = Duration::from_millis(50);
    for _ in 0..CLOSE_TIME.as_millis() / step.as_millis() {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return true;
        }
        std::thread::sleep(step);
    }
    false
}
