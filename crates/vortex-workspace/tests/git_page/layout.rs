//! The Git page's layout: long names cut short and the diff closed.

use std::time::Duration;

use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AppContext as _, ElementId, TestAppContext, px};

use super::super::project::*;

#[gpui_kit::test]
async fn a_long_branch_name_is_cut_short_in_its_row(cx: &mut TestAppContext) {
    let dir = project();
    let root = dir.path();
    let branch = format!("feature/{}", "very-long-name-".repeat(12));
    git(root, &["init", "-q", "-b", &branch]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "start"]);
    let (window, _) = open_workspace(cx, root.to_path_buf());
    let window = window.into();
    click(cx, window, "activity-git");
    cx.wait_for(window, Duration::from_secs(5), |window, _| {
        window.try_find("git-branch").is_some()
    })
    .await;
    let (list, refresh) = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let list = window.find("git-changes").bounds();
            (list, window.find("git-refresh").bounds())
        })
        .expect("window");
    assert!(
        refresh.right() <= list.right(),
        "the row's last button {refresh:?} stays in the list {list:?}"
    );
    click(cx, window, "git-branch");
    let item = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window
                .find(ElementId::Name(format!("menu-label-{branch}").into()))
                .bounds()
        })
        .expect("window");
    assert!(
        item.size.width <= px(320.),
        "cut short in the menu: {item:?}"
    );
}

#[gpui_kit::test]
async fn the_diff_closes_leaving_the_list_the_whole_page(cx: &mut TestAppContext) {
    let dir = changed_project();
    let (window, _) = open_workspace(cx, dir.path().to_path_buf());
    let window = window.into();
    click(cx, window, "activity-git");
    cx.wait_for(window, Duration::from_secs(5), |window, _| {
        window.try_find("git-diff").is_some()
    })
    .await;
    click(cx, window, "side-toggle-diff");
    assert!(!has(cx, window, "git-diff"), "closed");
    let (page, list) = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let page = window.find("git-page").bounds().size.width;
            (page, window.find("git-changes").bounds().size.width)
        })
        .expect("window");
    assert_eq!(list, page, "the list fills the page");
    click(cx, window, "side-toggle-list");
    assert!(has(cx, window, "git-diff"), "one side always stays");
    assert!(!has(cx, window, "git-changes"));
    click(cx, window, "side-toggle-diff");
    assert!(has(cx, window, "git-changes"), "the list again");
    assert!(!has(cx, window, "git-diff"));
}
