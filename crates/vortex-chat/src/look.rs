//! How every chat is laid out: which buttons and meters sit in the title
//! bar and which on the composer's strip, in what order, how the usage
//! meter reads and each harness's color. The window sets it from the
//! settings; the chat's customize mode changes it and reports the change.

use gpui_kit::{App, Global};

/// A button or meter that can be moved or hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Processes,
    Account,
    Usage,
    NewChat,
    Attach,
    Skills,
    Hooks,
    Mode,
    Model,
    Effort,
    Agents,
    Tokens,
    Cache,
    Context,
}

impl Item {
    pub const ALL: [Self; 14] = [
        Self::Processes,
        Self::Account,
        Self::Usage,
        Self::NewChat,
        Self::Attach,
        Self::Skills,
        Self::Hooks,
        Self::Mode,
        Self::Model,
        Self::Effort,
        Self::Agents,
        Self::Tokens,
        Self::Cache,
        Self::Context,
    ];

    /// Its name in the settings.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.names().0
    }

    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.name() == name)
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        self.names().1
    }

    /// Its name in the settings and the one shown.
    fn names(self) -> (&'static str, &'static str) {
        NAMES
            .iter()
            .find(|(item, ..)| *item == self)
            .map_or(("", ""), |(_, name, label)| (*name, *label))
    }
}

const NAMES: [(Item, &str, &str); 14] = [
    (Item::Processes, "processes", "Processes"),
    (Item::Account, "account", "Account"),
    (Item::Usage, "usage", "Usage"),
    (Item::NewChat, "new_chat", "New Conversation"),
    (Item::Attach, "attach", "Attach"),
    (Item::Skills, "skills", "Skills"),
    (Item::Hooks, "hooks", "Hooks"),
    (Item::Mode, "mode", "Mode"),
    (Item::Model, "model", "Model"),
    (Item::Effort, "effort", "Effort"),
    (Item::Agents, "agents", "Agents"),
    (Item::Tokens, "tokens", "Tokens"),
    (Item::Cache, "cache", "Prompt Cache"),
    (Item::Context, "context", "Context"),
];

/// Where items sit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// The title bar, after the conversation's name.
    Top,
    /// The composer's strip, from its left.
    BottomStart,
    /// The composer's strip, before the send button.
    BottomEnd,
}

impl Spot {
    pub const ALL: [Self; 3] = [Self::Top, Self::BottomStart, Self::BottomEnd];

    /// What customize mode calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Top => "Title bar",
            Self::BottomStart => "Composer, left",
            Self::BottomEnd => "Composer, right",
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::BottomStart => "bottom_start",
            Self::BottomEnd => "bottom_end",
        }
    }
}

/// What the usage meter reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UsageShown {
    /// The window nearer its limit.
    #[default]
    Fuller,
    Session,
    Weekly,
    Both,
}

impl UsageShown {
    pub const ALL: [Self; 4] = [Self::Fuller, Self::Session, Self::Weekly, Self::Both];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fuller => "fuller",
            Self::Session => "session",
            Self::Weekly => "weekly",
            Self::Both => "both",
        }
    }

    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|shown| shown.name() == name)
            .unwrap_or_default()
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fuller => "Nearest limit",
            Self::Session => "Session",
            Self::Weekly => "Weekly",
            Self::Both => "Both",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLook {
    pub top: Vec<Item>,
    pub bottom_start: Vec<Item>,
    pub bottom_end: Vec<Item>,
    pub usage: UsageShown,
    /// Each harness's color as `0xrrggbb`, `None` for its own.
    pub claude_color: Option<u32>,
    pub codex_color: Option<u32>,
    /// Whether the chats show their customize mode.
    pub editing: bool,
}

impl Default for ChatLook {
    fn default() -> Self {
        use Item::{
            Account, Agents, Attach, Cache, Context, Effort, Hooks, Mode, Model, NewChat,
            Processes, Skills, Tokens, Usage,
        };
        Self {
            top: vec![Processes, Account, Usage, NewChat],
            bottom_start: vec![Attach, Skills, Hooks, Mode, Model, Effort],
            bottom_end: vec![Agents, Tokens, Cache, Context],
            usage: UsageShown::Fuller,
            claude_color: None,
            codex_color: None,
            editing: false,
        }
    }
}

impl Global for ChatLook {}

impl ChatLook {
    /// The look the window set, or the default one.
    #[must_use]
    pub fn current(cx: &App) -> &Self {
        static DEFAULT: std::sync::LazyLock<ChatLook> = std::sync::LazyLock::new(ChatLook::default);
        cx.try_global::<Self>().unwrap_or(&DEFAULT)
    }

    #[must_use]
    pub fn items(&self, spot: Spot) -> &[Item] {
        match spot {
            Spot::Top => &self.top,
            Spot::BottomStart => &self.bottom_start,
            Spot::BottomEnd => &self.bottom_end,
        }
    }

    fn items_mut(&mut self, spot: Spot) -> &mut Vec<Item> {
        match spot {
            Spot::Top => &mut self.top,
            Spot::BottomStart => &mut self.bottom_start,
            Spot::BottomEnd => &mut self.bottom_end,
        }
    }

    /// The items in no spot.
    #[must_use]
    pub fn hidden(&self) -> Vec<Item> {
        let shown = |item: &Item| {
            Spot::ALL
                .iter()
                .any(|spot| self.items(*spot).contains(item))
        };
        Item::ALL.into_iter().filter(|item| !shown(item)).collect()
    }

    /// `item` moved to `spot`, before `before` or at its end.
    #[must_use]
    pub fn moved(&self, item: Item, spot: Spot, before: Option<Item>) -> Self {
        let mut look = self.without(item);
        let items = look.items_mut(spot);
        let at = before
            .and_then(|before| items.iter().position(|it| *it == before))
            .unwrap_or(items.len());
        items.insert(at, item);
        look
    }

    /// The color the user gave `brand`, if any.
    #[must_use]
    pub const fn color(&self, brand: crate::Brand) -> Option<u32> {
        match brand {
            crate::Brand::Claude => self.claude_color,
            crate::Brand::Codex => self.codex_color,
            crate::Brand::Vortex => None,
        }
    }

    /// With `color` for `brand`; Vortex's own takes none, as a chat is
    /// always on a harness.
    #[must_use]
    pub fn colored(&self, brand: crate::Brand, color: Option<u32>) -> Self {
        let mut look = self.clone();
        match brand {
            crate::Brand::Claude => look.claude_color = color,
            crate::Brand::Codex => look.codex_color = color,
            crate::Brand::Vortex => {}
        }
        look
    }

    /// Where `item` sits and its place there.
    #[must_use]
    pub fn place(&self, item: Item) -> Option<(Spot, usize)> {
        Spot::ALL.into_iter().find_map(|spot| {
            let at = self.items(spot).iter().position(|it| *it == item)?;
            Some((spot, at))
        })
    }

    /// `item` one place earlier, or later with `later`, in its spot.
    #[must_use]
    pub fn shifted(&self, item: Item, later: bool) -> Self {
        let mut look = self.clone();
        let Some((spot, at)) = self.place(item) else {
            return look;
        };
        let items = look.items_mut(spot);
        let to = if later { at + 1 } else { at.wrapping_sub(1) };
        if to < items.len() {
            items.swap(at, to);
        }
        look
    }

    /// `item` hidden.
    #[must_use]
    pub fn without(&self, item: Item) -> Self {
        let mut look = self.clone();
        for spot in Spot::ALL {
            look.items_mut(spot).retain(|it| *it != item);
        }
        look
    }
}

#[cfg(test)]
mod tests {
    use super::{ChatLook, Item, Spot, UsageShown};

    #[test]
    fn an_item_moves_between_spots_and_hides() {
        let look = ChatLook::default();
        let moved = look.moved(Item::Usage, Spot::BottomEnd, Some(Item::Context));
        assert!(!moved.top.contains(&Item::Usage));
        assert_eq!(
            moved.bottom_end,
            [
                Item::Agents,
                Item::Tokens,
                Item::Cache,
                Item::Usage,
                Item::Context
            ]
        );
        let back = moved.moved(Item::Usage, Spot::Top, None);
        assert_eq!(back.top.last(), Some(&Item::Usage));
        assert_eq!(look.hidden(), []);
        assert_eq!(look.without(Item::Tokens).hidden(), [Item::Tokens]);
        let shifted = look.shifted(Item::Account, false);
        assert_eq!(shifted.top[..2], [Item::Account, Item::Processes]);
        assert_eq!(
            shifted.shifted(Item::Account, false),
            shifted,
            "already first"
        );
        assert_eq!(look.shifted(Item::NewChat, true), look, "already last");
        assert_eq!(look.place(Item::Mode), Some((Spot::BottomStart, 3)));
    }

    #[test]
    fn names_go_both_ways() {
        for item in Item::ALL {
            assert_eq!(Item::named(item.name()), Some(item));
        }
        assert_eq!(Item::named("nope"), None);
        assert_eq!(UsageShown::named("both"), UsageShown::Both);
        assert_eq!(UsageShown::named("nope"), UsageShown::Fuller);
    }
}
