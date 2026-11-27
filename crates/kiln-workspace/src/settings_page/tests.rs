use std::path::Path;

use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowHandle, WindowOptions, px,
    size,
};
use kiln_settings::Scope;
use serde_json::json;

use super::{SettingsPage, SettingsPageEvent};

/// The page alone in a window, on the project tab, so nothing is written to
/// the user's own settings.
fn page(cx: &mut TestAppContext, root: &Path) -> WindowHandle<SettingsPage> {
    cx.update(gpui_kit::init);
    cx.update(crate::config::init);
    let root = root.to_owned();
    let window = cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                gpui_kit::Point::default(),
                size(px(1000.), px(800.)),
            ))),
            ..Default::default()
        };
        cx.open_window(options, |_, cx| {
            cx.new(|cx| {
                let layers = kiln_settings::Layers::read(Some(&root), serde_json::Value::Null);
                let mut page = SettingsPage::new(layers.expect("layers"), cx);
                page.pick_scope(Scope::Project, cx);
                page
            })
        })
        .expect("window")
    });
    cx.run_until_parked();
    window
}

fn shown(cx: &mut TestAppContext, window: WindowHandle<SettingsPage>, label: &str) -> bool {
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.try_find_by_label(label).is_some()
    })
    .expect("window")
}

fn entry(key: &str) -> &'static kiln_settings::Entry {
    kiln_settings::find(key).expect("in the catalog")
}

#[gpui_kit::test]
fn a_change_is_written_to_the_tabs_file_and_announced(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = page(cx, dir.path());
    let entity: Entity<SettingsPage> = window.entity(cx).expect("page");
    let changes = std::rc::Rc::new(std::cell::Cell::new(0));
    let counted = changes.clone();
    cx.update(|cx| {
        cx.subscribe(&entity, move |_, event: &SettingsPageEvent, _| {
            if matches!(event, SettingsPageEvent::Changed) {
                counted.set(counted.get() + 1);
            }
        })
        .detach();
    });
    assert!(shown(cx, window, "Interface font size"));
    entity.update(cx, |page, cx| {
        page.save(entry("ui.font_size"), Some(json!(17)), cx);
    });
    let file = dir.path().join(".kiln/settings.json");
    let written = kiln_settings::read_layer(&file).expect("read");
    assert_eq!(written["ui"]["font_size"], json!(17));
    assert_eq!(changes.get(), 1);
    let described = entity.read_with(cx, |page, _| {
        super::fields::described(entry("ui.font_size"), page)
    });
    assert_eq!(
        described.as_deref(),
        Some("Every part of the interface scales with it. Set here.")
    );

    entity.update(cx, |page, cx| {
        page.save(entry("ui.font_size"), Some(json!(500)), cx);
    });
    assert_eq!(changes.get(), 1, "a value out of range is refused");
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("settings-error").is_some());
    })
    .expect("window");

    entity.update(cx, |page, cx| page.save(entry("ui.font_size"), None, cx));
    let written = kiln_settings::read_layer(&file).expect("read");
    assert_eq!(written.get("ui"), None, "taken out again");
}
