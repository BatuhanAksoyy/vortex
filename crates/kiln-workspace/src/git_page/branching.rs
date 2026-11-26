//! The branch over the list, as a menu of the local branches to switch to,
//! and a box to name a new one.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, Context, Entity, FontWeight, KeyDownEvent, SharedString, Subscription, TestSupportExt,
    WeakEntity, Window,
    assets::IconName as Lucide,
    component::{
        Icon, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, InputEvent, InputState},
        menu::{DropdownMenu as _, PopupMenuItem},
    },
    div, px,
};
use kiln_chat::scale::{ScaledButton as _, ScaledIcon as _};
use kiln_git::Branch;

use super::{GitPage, committing::Done};

const MENU_WIDTH: f32 = 260.;
const MENU_HEIGHT: f32 = 420.;

/// The box naming a new branch, while it is open.
pub(super) struct NewBranch {
    name: Entity<InputState>,
    _typed: Subscription,
}

impl GitPage {
    pub(super) fn read_branches(&mut self, cx: &mut Context<Self>) {
        let root = self
            .repo
            .read(cx)
            .status()
            .map(|status| status.root.clone());
        let Some(root) = root else {
            return;
        };
        let read = cx.background_spawn(async move { kiln_git::branches(&root) });
        self.branch_load = Some(cx.spawn(async move |this, cx| {
            let Ok(branches) = read.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.show_branches(branches, cx));
        }));
    }

    fn show_branches(&mut self, branches: Vec<Branch>, cx: &mut Context<Self>) {
        if self.branches != branches {
            self.branches = branches;
            cx.notify();
        }
    }

    fn switch_branch(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.write(window, cx, Done::Staging, move |root| {
            kiln_git::switch(root, &name)
        });
    }

    fn open_new_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("New branch name"));
        let typed = cx.subscribe_in(&name, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.create_branch(window, cx);
            }
        });
        name.update(cx, |name, cx| name.focus(window, cx));
        self.new_branch = Some(NewBranch {
            name,
            _typed: typed,
        });
        cx.notify();
    }

    fn create_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(new) = &self.new_branch else {
            return;
        };
        let name = new.name.read(cx).value().to_string();
        self.write(window, cx, Done::Branched, move |root| {
            kiln_git::create_branch(root, &name)
        });
    }

    fn close_new_branch(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            self.new_branch = None;
            self.focus(window, cx);
            cx.stop_propagation();
            cx.notify();
        }
    }

    /// The branch checked out; a click lists the others to switch to.
    pub(super) fn render_branch_menu(&self, branch: &str, cx: &Context<Self>) -> impl IntoElement {
        let branches = self.branches.clone();
        let view = cx.entity().downgrade();
        Button::new("git-branch")
            .ghost()
            .control()
            .min_w_0()
            .accessibility_label(SharedString::from(branch.to_owned()))
            .tooltip("Switch Branch")
            .child(
                h_flex()
                    .gap_1()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(branch.to_owned()),
                    )
                    .child(Icon::new(Lucide::ChevronDown).in_row()),
            )
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _, _| {
                let menu = menu
                    .min_w(px(MENU_WIDTH))
                    .max_h(px(MENU_HEIGHT))
                    .scrollable(true)
                    .label("Branches");
                let menu = branches
                    .iter()
                    .fold(menu, |menu, branch| menu.item(branch_item(branch, &view)));
                let create = view.clone();
                menu.separator().item(
                    PopupMenuItem::new("Create Branch…")
                        .icon(Icon::new(Lucide::Plus))
                        .on_click(move |_, window, cx| {
                            let _ = create.update(cx, |this, cx| this.open_new_branch(window, cx));
                        }),
                )
            })
    }

    pub(super) fn render_new_branch(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let new = self.new_branch.as_ref()?;
        Some(
            div()
                .id("git-new-branch")
                .test_support()
                .on_key_down(cx.listener(Self::close_new_branch))
                .child(Input::new(&new.name).small()),
        )
    }
}

fn branch_item(branch: &Branch, view: &WeakEntity<GitPage>) -> PopupMenuItem {
    let (name, view) = (branch.name.clone(), view.clone());
    PopupMenuItem::new(branch.name.clone())
        .checked(branch.current)
        .on_click(move |_, window, cx| {
            let name = name.clone();
            let _ = view.update(cx, |this, cx| this.switch_branch(name, window, cx));
        })
}
