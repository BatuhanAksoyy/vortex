use std::{collections::HashSet, path::PathBuf, sync::Arc};

use gpui_kit::prelude::*;
use gpui_kit::{
    App, ClickEvent, Context, EventEmitter, FocusHandle, Focusable, SharedString, TestSupportExt,
    WeakEntity, Window,
    component::{
        ActiveTheme, IconName, h_flex,
        list::ListItem,
        tree::{TreeItem, TreeState, tree},
    },
    rems,
};
use kiln_project::TreeNode;

const KEY_CONTEXT: &str = "FileTree";

pub enum FileTreeEvent {
    /// A file was activated; the path is relative to the project root.
    Open(PathBuf),
}

pub struct FileTree {
    state: gpui_kit::Entity<TreeState>,
    /// Directory ids, so empty directories (which have no children in the
    /// tree component) still render and behave as directories.
    dirs: Arc<HashSet<SharedString>>,
    focus_handle: FocusHandle,
}

impl EventEmitter<FileTreeEvent> for FileTree {}

impl FileTree {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            state: cx.new(|cx| TreeState::new(cx)),
            dirs: Arc::default(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn set_nodes(&mut self, nodes: &[TreeNode], cx: &mut Context<Self>) {
        let mut dirs = HashSet::new();
        collect_dirs(nodes, &mut dirs);
        self.dirs = Arc::new(dirs);
        let items = to_items(nodes);
        self.state
            .update(cx, |state, cx| state.set_items(items, cx));
    }
}

fn to_items(nodes: &[TreeNode]) -> Vec<TreeItem> {
    nodes
        .iter()
        .map(|node| {
            let item = TreeItem::new(node.path.to_string_lossy().into_owned(), node.name.clone());
            if node.is_dir {
                item.children(to_items(&node.children))
            } else {
                item
            }
        })
        .collect()
}

/// Click handler for a row: files open, directories only expand.
fn open_on_click(
    tree: WeakEntity<FileTree>,
    path: Option<PathBuf>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        if let Some(path) = path.clone() {
            let _ = tree.update(cx, |_, cx| cx.emit(FileTreeEvent::Open(path)));
        }
    }
}

fn collect_dirs(nodes: &[TreeNode], dirs: &mut HashSet<SharedString>) {
    for node in nodes.iter().filter(|node| node.is_dir) {
        dirs.insert(node.path.to_string_lossy().into_owned().into());
        collect_dirs(&node.children, dirs);
    }
}

/// Indentation per depth level, in rems so it follows the UI zoom.
fn indent(depth: usize) -> gpui_kit::Rems {
    let depth = f32::from(u16::try_from(depth).unwrap_or(u16::MAX));
    rems(0.5 + depth * 0.875)
}

impl Focusable for FileTree {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FileTree {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let dirs = self.dirs.clone();
        let rows = tree(&self.state, move |ix, entry, selected, _, cx| {
            let item = entry.item().clone();
            let is_dir = entry.is_folder() || dirs.contains(&item.id);
            let icon = match (is_dir, entry.is_expanded()) {
                (false, _) => IconName::File,
                (true, true) => IconName::FolderOpen,
                (true, false) => IconName::Folder,
            };
            let open_path = (!is_dir).then(|| PathBuf::from(item.id.as_ref()));
            ListItem::new(ix)
                .selected(selected)
                .rounded(cx.theme().radius)
                .py_0p5()
                .pl(indent(entry.depth()))
                .child(h_flex().gap_2().child(icon).child(item.label.clone()))
                .on_click(open_on_click(this.clone(), open_path))
        });
        gpui_kit::div()
            .id("file-tree")
            .test_support()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .p_1()
            .text_sm()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .child(rows)
    }
}
