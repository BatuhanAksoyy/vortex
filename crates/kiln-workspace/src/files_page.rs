//! The Files page, which the side panel shows beside the chats: the
//! project's files as a tree beside the editor.

mod operations;
mod split;
mod tree;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    rc::Rc,
};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, EventEmitter, FontWeight, KeyDownEvent, ScrollStrategy, SharedString,
    Subscription, TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants as _},
        h_flex,
        resizable::{ResizableState, h_resizable, resizable_panel},
        tree::{TreeEvent, TreeItem, TreeState},
        v_flex,
    },
    div,
};
use kiln_chat::scale::ScaledButton as _;
use kiln_project::TreeNode;

use crate::{editor_pane::EditorPane, repo_status::RepoStatus};

const TREE_WIDTH_REMS: f32 = 15.0;

/// The project's files need listing again.
pub struct FilesChanged;

impl EventEmitter<FilesChanged> for FilesPage {}

pub struct FilesPage {
    root: PathBuf,
    tree: Entity<TreeState>,
    editor: Entity<EditorPane>,
    repo: Entity<RepoStatus>,
    /// What the tree shows, so the same listing leaves it alone.
    nodes: Vec<TreeNode>,
    folders: Rc<HashSet<SharedString>>,
    /// The folders open, kept when the files are listed again.
    expanded: HashSet<SharedString>,
    /// A file to select that the tree did not list yet.
    pending: Option<PathBuf>,
    /// The file last selected because the editor showed it.
    followed: Option<PathBuf>,
    tree_hidden: bool,
    /// Held here, not by the window, so the split the user sized outlives
    /// the panel being closed.
    tree_split: Entity<ResizableState>,
    /// The same, while the tree is on the right.
    split_right: Entity<ResizableState>,
    split: split::Split,
    _subscriptions: [Subscription; 3],
}

impl FilesPage {
    pub fn new(
        root: PathBuf,
        [editor, aside]: [Entity<EditorPane>; 2],
        repo: Entity<RepoStatus>,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree = cx.new(|cx| TreeState::new(cx));
        let subscriptions = [
            cx.observe(&editor, |this, _, cx| this.follow_editor(cx)),
            cx.observe(&repo, |_, _, cx| cx.notify()),
            cx.subscribe(&tree, |this, _, event: &TreeEvent, _| match event {
                TreeEvent::Expanded(id) => drop(this.expanded.insert(id.clone())),
                TreeEvent::Collapsed(id) => drop(this.expanded.remove(id)),
            }),
        ];
        Self {
            root,
            tree,
            editor,
            repo,
            nodes: Vec::new(),
            folders: Rc::default(),
            expanded: HashSet::new(),
            pending: None,
            followed: None,
            tree_hidden: false,
            tree_split: cx.new(|_| ResizableState::default()),
            split_right: cx.new(|_| ResizableState::default()),
            split: split::Split::new(aside, cx),
            _subscriptions: subscriptions,
        }
    }

    pub fn set_files(&mut self, nodes: Vec<TreeNode>, cx: &mut Context<Self>) {
        if self.nodes != nodes {
            let mut folders = HashSet::new();
            collect_folders(&nodes, &mut folders);
            self.folders = Rc::new(folders);
            self.nodes = nodes;
            self.show_nodes(cx);
        }
        if let Some(pending) = self.pending.take() {
            self.reveal(&pending, cx);
        }
    }

    fn show_nodes(&self, cx: &mut Context<Self>) {
        let items = items(&self.nodes, &self.expanded);
        self.tree.update(cx, |tree, cx| {
            let selected = tree.selected_item().map(|item| item.id.clone());
            tree.set_items(items, cx);
            let index = selected.and_then(|id| tree.index_of(&id));
            tree.set_selected_index(index, cx);
        });
    }

    fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.expanded.clear();
        self.show_nodes(cx);
        cx.notify();
    }

    /// Hides the tree, leaving the editor, or shows it again.
    pub fn toggle_tree(&mut self, cx: &mut Context<Self>) {
        self.tree_hidden = !self.tree_hidden;
        cx.notify();
    }

    /// Selects `relative` in the tree, opening the folders above it.
    pub fn reveal(&mut self, relative: &Path, cx: &mut Context<Self>) {
        let id = SharedString::from(relative.to_string_lossy().into_owned());
        let listed = self.tree.update(cx, |tree, cx| {
            tree.reveal_item(&id, ScrollStrategy::Nearest, cx);
            let index = tree.index_of(&id);
            tree.set_selected_index(index, cx);
            index.is_some()
        });
        self.pending = (!listed).then(|| relative.to_path_buf());
    }

    /// Selects the file the editor switched to.
    fn follow_editor(&mut self, cx: &mut Context<Self>) {
        let active = self.active_editor(cx).read(cx).active_path(cx);
        let active = active.map(Path::to_path_buf);
        if active != self.followed {
            self.followed.clone_from(&active);
            let relative = active
                .as_deref()
                .and_then(|p| p.strip_prefix(&self.root).ok());
            if let Some(relative) = relative {
                self.reveal(relative, cx);
            }
        }
        cx.notify();
    }

    fn open(&mut self, relative: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.root.join(relative);
        self.open_in_editor(path, None, window, cx);
    }

    fn is_folder(&self, id: &str) -> bool {
        self.folders.contains(id)
    }

    fn selected(&self, cx: &Context<Self>) -> Option<PathBuf> {
        let item = self.tree.read(cx).selected_item()?;
        Some(PathBuf::from(item.id.as_ref()))
    }

    fn on_tree_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.selected(cx) else {
            return;
        };
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "enter" if !self.is_folder(&selected.to_string_lossy()) => {
                self.open(&selected, window, cx);
            }
            "f2" => self.ask_rename(&selected, window, cx),
            "delete" => self.ask_delete(&selected, window, cx),
            "backspace" if keystroke.modifiers.platform => self.ask_delete(&selected, window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn header(&self, cx: &Context<Self>) -> impl IntoElement {
        let name = self.root.file_name().unwrap_or(self.root.as_os_str());
        let button = |id, icon, tooltip| {
            let button = Button::new(id).ghost();
            button.control_icon(icon).tooltip(tooltip)
        };
        h_flex()
            .id("files-header")
            .test_support()
            .flex_none()
            .gap_0p5()
            .pl_3()
            .pr_1()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name.to_string_lossy().into_owned()),
            )
            .child(
                button("files-new-file", Lucide::FilePlus, "New File…").on_click(cx.listener(
                    |this, _, window, cx| this.ask_new_file(&this.target(cx), window, cx),
                )),
            )
            .child(
                button("files-new-folder", Lucide::FolderPlus, "New Folder…").on_click(
                    cx.listener(|this, _, window, cx| {
                        this.ask_new_folder(&this.target(cx), window, cx);
                    }),
                ),
            )
            .child(
                button("files-refresh", Lucide::RefreshCw, "Refresh")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(FilesChanged))),
            )
            .child(
                button(
                    "files-collapse-all",
                    Lucide::CopyMinus,
                    "Collapse All Folders",
                )
                .on_click(cx.listener(|this, _, _, cx| this.collapse_all(cx))),
            )
    }

    fn side(&self, cx: &Context<Self>) -> impl IntoElement {
        let right = crate::config::settings(cx).layout.file_tree == kiln_settings::Side::Right;
        v_flex()
            .id("files-tree")
            .test_support()
            .size_full()
            .map(|tree| {
                if right {
                    tree.border_l_1()
                } else {
                    tree.border_r_1()
                }
            })
            .border_color(cx.theme().border)
            .on_key_down(cx.listener(Self::on_tree_key))
            .child(self.header(cx))
            .child(div().flex_1().min_h_0().py_1().child(self.render_tree(cx)))
    }
}

/// The tree's items, keyed by path.
fn items(nodes: &[TreeNode], expanded: &HashSet<SharedString>) -> Vec<TreeItem> {
    nodes
        .iter()
        .map(|node| {
            let id = node_id(node);
            let open = expanded.contains(&id);
            TreeItem::new(id, node.name.clone())
                .children(items(&node.children, expanded))
                .expanded(open)
        })
        .collect()
}

fn collect_folders(nodes: &[TreeNode], folders: &mut HashSet<SharedString>) {
    for node in nodes.iter().filter(|node| node.is_dir) {
        folders.insert(node_id(node));
        collect_folders(&node.children, folders);
    }
}

fn node_id(node: &TreeNode) -> SharedString {
    node.path.to_string_lossy().into_owned().into()
}

impl Render for FilesPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let editor = if self.editor.read(cx).is_empty() {
            v_flex()
                .id("files-empty")
                .test_support()
                .size_full()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Select a file to open it.")
                .into_any_element()
        } else {
            self.editor.clone().into_any_element()
        };
        let editor = self.render_editors(editor, cx);
        let side = (!self.tree_hidden).then(|| self.side(cx));
        let tree = resizable_panel()
            .size(window.rem_size() * TREE_WIDTH_REMS)
            .fixed()
            .visible(!self.tree_hidden)
            .children(side);
        let editor = resizable_panel().child(editor);
        if crate::config::settings(cx).layout.file_tree == kiln_settings::Side::Right {
            // A split of its own: the sizes are kept by their order.
            return h_resizable("files-page-tree-right")
                .with_state(&self.split_right)
                .child(editor)
                .child(tree);
        }
        h_resizable("files-page")
            .with_state(&self.tree_split)
            .child(tree)
            .child(editor)
    }
}
