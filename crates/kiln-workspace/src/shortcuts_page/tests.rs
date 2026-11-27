use std::path::PathBuf;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowHandle, WindowOptions, px,
    size,
};

use super::ShortcutsPage;

const QUICK_OPEN: &str = "workbench.action.quickOpen";

/// The page alone in a window, over a file of its own, so the user's own
/// keybindings are never written.
fn page(cx: &mut TestAppContext, file: PathBuf) -> WindowHandle<ShortcutsPage> {
    cx.update(gpui_kit::init);
    cx.update(crate::config::init);
    let window = cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                gpui_kit::Point::default(),
                size(px(1000.), px(800.)),
            ))),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            cx.new(|cx| ShortcutsPage::new(file, window, cx))
        })
        .expect("window")
    });
    cx.run_until_parked();
    window
}

fn click(cx: &mut TestAppContext, window: WindowHandle<ShortcutsPage>, id: &str) {
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(gpui_kit::SharedString::from(id.to_owned()), cx);
    })
    .expect("window");
    cx.run_until_parked();
}

fn keys(cx: &mut TestAppContext, page: &Entity<ShortcutsPage>) -> Vec<String> {
    page.read_with(cx, |page, _| {
        page.keys(QUICK_OPEN)
            .into_iter()
            .map(str::to_owned)
            .collect()
    })
}

#[gpui_kit::test]
fn pressed_keys_replace_a_commands_shortcut_until_it_is_reset(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("keybindings.json");
    let window = page(cx, file.clone());
    let page: Entity<ShortcutsPage> = window.entity(cx).expect("page");
    assert_eq!(keys(cx, &page).len(), 1, "the default");

    click(cx, window, &format!("shortcut-change-{QUICK_OPEN}"));
    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-alt-o", cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(keys(cx, &page), ["ctrl-alt-o"]);
    let written = std::fs::read_to_string(&file).expect("written");
    assert!(written.contains("ctrl+alt+o"), "{written}");

    click(cx, window, &format!("shortcut-reset-{QUICK_OPEN}"));
    assert_eq!(keys(cx, &page).len(), 1);
    assert_ne!(keys(cx, &page), ["ctrl-alt-o"], "the default again");

    click(cx, window, &format!("shortcut-remove-{QUICK_OPEN}"));
    assert_eq!(keys(cx, &page), [] as [&str; 0]);
}

#[gpui_kit::test]
fn the_search_finds_commands_by_name(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = page(cx, dir.path().join("keybindings.json"));
    let page: Entity<ShortcutsPage> = window.entity(cx).expect("page");
    let search = page.read_with(cx, |page, _| page.search.clone());
    cx.update_window(window.into(), |_, window, cx| {
        search.update(cx, |search, cx| search.set_value("split", window, cx));
    })
    .expect("window");
    cx.run_until_parked();
    let shown = page.read_with(cx, |page, cx| {
        page.shown(cx)
            .iter()
            .map(|spec| spec.id)
            .collect::<Vec<_>>()
    });
    assert_eq!(shown, ["workbench.action.splitEditor"]);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find("shortcut-workbench.action.splitEditor")
                .is_some()
        );
        assert!(
            window
                .try_find("shortcut-workbench.action.quickOpen")
                .is_none()
        );
    })
    .expect("window");
}
