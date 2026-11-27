use std::path::Path;

use gpui_kit::prelude::*;
use gpui_kit::test::{TestAppContextExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, TestAppContext, Window, WindowBounds, WindowOptions, div, px,
    size,
};
use serde_json::{Value, json};
use vortex_settings::Scope;

use super::{Opened, open};

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The page on the project tab of `root`, alone in a window: with no
/// workspace, no watcher follows the files it writes.
fn page(cx: &mut TestAppContext, root: &Path) -> AnyWindowHandle {
    cx.update(gpui_kit::init);
    cx.update(crate::config::init);
    let (window, _) = cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                gpui_kit::Point::default(),
                size(px(1000.), px(800.)),
            ))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Blank)).expect("window")
    });
    let opened = Opened {
        scope: Scope::Project,
        project: Some(root.to_path_buf()),
        chat: None,
        own: Value::Null,
        agents: vec!["claude".to_owned()],
        models: Vec::new(),
    };
    cx.update_window(window, |_, window, cx| open(opened, window, cx))
        .expect("window");
    // The sheet slides in; clicks land once it is in place.
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    window
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    })
    .expect("window");
    cx.run_until_parked();
}

fn written(root: &Path) -> Value {
    vortex_settings::read_layer(&root.join(".vortex/settings.json")).expect("read")
}

#[gpui_kit::test]
async fn the_project_tab_writes_the_projects_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir(root.join(".vortex")).expect("mkdir");
    let kept = "{\n  // Ours.\n  \"git\": { \"commit\": { \"trigger\": \"agent\" } }\n}\n";
    std::fs::write(root.join(".vortex/settings.json"), kept).expect("write");
    let window = page(cx, root);
    cx.wait_for(window, std::time::Duration::from_secs(5), |window, _| {
        window.try_find("commit-settings").is_some()
    })
    .await;

    click(cx, window, "commit-settings-enabled");
    let commit = &written(root)["git"]["commit"];
    assert_eq!(commit["enabled"], json!(true));
    assert_eq!(commit["trigger"], "agent", "the rest stays");
    let text = std::fs::read_to_string(root.join(".vortex/settings.json")).expect("read");
    assert!(text.contains("// Ours."), "and its comments: {text}");

    click(cx, window, "commit-settings-reset-enabled");
    assert_eq!(written(root)["git"]["commit"].get("enabled"), None);
}
