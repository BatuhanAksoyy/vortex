//! The rows of the file tree and their menu.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    App, ClipboardItem, Context, FontWeight, SharedString, TestSupportExt, WeakEntity, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Icon, Sizable as _, h_flex,
        list::ListItem,
        menu::{PopupMenu, PopupMenuItem},
        tooltip::Tooltip,
        tree::{TreeEntry, tree},
    },
    div, px,
};
use kiln_git::Kind;

use super::FilesPage;
use crate::{file_icon, repo_status::kind_color};

/// How far each level of the tree is indented.
const INDENT: f32 = 14.0;

impl FilesPage {
    pub(super) fn render_tree(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let page = cx.entity().downgrade();
        let menu_page = page.clone();
        let repo = self.repo.clone();
        let folders = self.folders.clone();
        tree(&self.tree, move |_, entry, _, _, cx| {
            let item = entry.item();
            let relative = PathBuf::from(item.id.as_ref());
            let mark = repo.read(cx).mark(&relative);
            let folder = folders.contains(&item.id);
            let row = row(entry, folder, mark, cx);
            if folder {
                return row;
            }
            let page = page.clone();
            row.on_click(move |_, window, cx| {
                let _ = page.update(cx, |page, cx| page.open(&relative, window, cx));
            })
        })
        .context_menu(move |_, entry, menu, _, _| {
            let relative = PathBuf::from(entry.item().id.as_ref());
            Self::menu(&relative, &menu_page, menu)
        })
    }

    fn menu(relative: &Path, page: &WeakEntity<Self>, menu: PopupMenu) -> PopupMenu {
        let item = |label: &'static str, action: Action| {
            let (page, relative) = (page.clone(), relative.to_path_buf());
            PopupMenuItem::new(label).on_click(move |_, window, cx| {
                let _ = page.update(cx, |page, cx| action(page, &relative, window, cx));
            })
        };
        let reveal = if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else {
            "Open Containing Folder"
        };
        let menu = menu
            .item(item("New File…", |page, at, window, cx| {
                page.ask_new_file(&page.folder_of(at), window, cx);
            }))
            .item(item("New Folder…", |page, at, window, cx| {
                page.ask_new_folder(&page.folder_of(at), window, cx);
            }))
            .separator()
            .item(item(reveal, |page, at, _, cx| {
                cx.reveal_path(&page.root.join(at));
            }))
            .item(item("Copy Path", |page, at, _, cx| {
                copy(page.root.join(at).display().to_string(), cx);
            }))
            .item(item("Copy Relative Path", |_, at, _, cx| {
                copy(at.display().to_string(), cx);
            }))
            .separator();
        menu.item(item("Rename…", FilesPage::ask_rename))
            .item(item("Delete", FilesPage::ask_delete))
    }
}

type Action = fn(&mut FilesPage, &Path, &mut Window, &mut Context<FilesPage>);

fn copy(text: String, cx: &App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
}

fn row(entry: &TreeEntry, folder: bool, mark: Option<Kind>, cx: &App) -> ListItem {
    let item = entry.item();
    let color = mark.map(|kind| kind_color(kind, cx.theme()));
    let icon = if folder {
        file_icon::folder(&item.label, entry.is_expanded(), cx)
    } else {
        file_icon::file(&item.label, cx)
    };
    let chevron = match (folder, entry.is_expanded()) {
        (true, true) => Some(Lucide::ChevronDown),
        (true, false) => Some(Lucide::ChevronRight),
        (false, _) => None,
    };
    let muted = cx.theme().muted_foreground;
    let depth = f32::from(u16::try_from(entry.depth()).unwrap_or(u16::MAX));
    ListItem::new(SharedString::from(format!("file-{}", item.id)))
        .pl(px(depth * INDENT + 4.))
        .py_0p5()
        .text_sm()
        .child(
            h_flex()
                .gap_1p5()
                .child(
                    div().flex_none().size_4().children(
                        chevron.map(|chevron| Icon::new(chevron).small().text_color(muted)),
                    ),
                )
                .child(icon)
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .when_some(color, Styled::text_color)
                        .child(item.label.clone()),
                ),
        )
        .when_some(mark.zip(color), |row, (kind, color)| {
            let id = format!("file-mark-{}", item.id);
            row.suffix(move |_, _| mark_view(&id, kind, folder).text_color(color))
        })
}

/// A changed file's letter, or a dot on a folder that holds changes.
fn mark_view(id: &str, kind: Kind, folder: bool) -> impl Styled + IntoElement + use<> {
    let (mark, tip) = if folder {
        ("●".to_owned(), "Contains changes")
    } else {
        (kind.letter().to_string(), kind.label())
    };
    div()
        .id(SharedString::from(id.to_owned()))
        .test_support()
        .flex_none()
        .pl_2()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .child(mark)
}
