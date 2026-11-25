//! What the site keeps in the browser: cookies, local and session storage,
//! and `IndexedDB`; each can be looked at and removed.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, SharedString, Task, TestSupportExt,
    component::{
        ActiveTheme, IconName, Sizable as _, Theme,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    div, px,
};
use kiln_browser::{Cookie, Database, Page, StoreEntries};
use kiln_chat::scale::{ScaledButton as _, ScaledText as _};

use super::{Browser, site_permissions::origin, tools_panel::Tool};

const PLACES_WIDTH: f32 = 168.;
const KEY_WIDTH: f32 = 120.;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Place {
    #[default]
    Cookies,
    Local,
    Session,
    Store {
        database: String,
        store: String,
    },
}

enum Kept {
    Cookies(Vec<Cookie>),
    Pairs(Vec<(String, String)>),
    Entries(StoreEntries),
}

#[derive(Default)]
pub(super) struct Storage {
    place: Place,
    databases: Vec<Database>,
    /// What the place holds; `None` while it is read.
    kept: Option<Result<Kept, String>>,
    reading: Option<Task<()>>,
}

impl Browser {
    pub(super) fn read_storage(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.page().cloned() else {
            self.storage.kept = Some(Err("The page is not open.".to_owned()));
            return;
        };
        let place = self.storage.place.clone();
        let url = self.url.clone();
        self.storage.kept = None;
        self.storage.reading = Some(cx.spawn(async move |this, cx| {
            let databases = page.databases().await.unwrap_or_default();
            let kept = read(&page, &place, &url).await;
            let _ = this.update(cx, |this, cx| {
                this.storage.databases = databases;
                this.storage.kept = Some(kept.map_err(|error| format!("{error:#}")));
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The document shown keeps other things.
    pub(super) fn storage_changed(&mut self, cx: &mut Context<Self>) {
        if self.showing_tool(Tool::Storage) {
            self.read_storage(cx);
        }
    }

    fn go_to_place(&mut self, place: Place, cx: &mut Context<Self>) {
        self.storage.place = place;
        self.read_storage(cx);
    }

    fn remove_kept(&mut self, key: String, cx: &mut Context<Self>) {
        let Some(page) = self.page().cloned() else {
            return;
        };
        let place = self.storage.place.clone();
        let cookie = match &self.storage.kept {
            Some(Ok(Kept::Cookies(cookies))) => cookies.iter().find(|it| it.name == key).cloned(),
            _ => None,
        };
        self.storage.reading = Some(cx.spawn(async move |this, cx| {
            let removed = match (&place, cookie) {
                (Place::Cookies, Some(cookie)) => page.delete_cookie(&cookie).await,
                (Place::Local | Place::Session, _) => {
                    page.remove_web_storage(place == Place::Local, &key).await
                }
                _ => Ok(()),
            };
            if let Err(error) = removed {
                log::warn!("the page's storage was not changed: {error:#}");
            }
            let _ = this.update(cx, Browser::read_storage);
        }));
    }

    fn clear_site(&mut self, cx: &mut Context<Self>) {
        let (Some(page), Some(origin)) = (self.page().cloned(), origin(&self.url)) else {
            return;
        };
        self.storage.reading = Some(cx.spawn(async move |this, cx| {
            if let Err(error) = page.clear_site_data(&origin).await {
                log::warn!("the site's data was not cleared: {error:#}");
            }
            let _ = this.update(cx, Browser::read_storage);
        }));
    }

    pub(super) fn storage_actions(cx: &Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .gap_1()
            .child(
                Button::new("browser-storage-refresh")
                    .ghost()
                    .small()
                    .label("Refresh")
                    .on_click(cx.listener(|this, _, _, cx| this.read_storage(cx))),
            )
            .child(
                Button::new("browser-storage-clear")
                    .ghost()
                    .small()
                    .label("Clear Site Data")
                    .tooltip("Remove the site's cookies, storage, databases and caches")
                    .on_click(cx.listener(|this, _, _, cx| this.clear_site(cx))),
            )
    }

    pub(super) fn render_storage(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let rows = match &self.storage.kept {
            None => vec![muted("Reading…", theme)],
            Some(Err(error)) => vec![muted(&format!("Cannot read it: {error}"), theme)],
            Some(Ok(kept)) => self.kept_rows(kept, cx),
        };
        h_flex()
            .size_full()
            .items_start()
            .text_caption()
            .child(self.places(cx))
            .child(
                v_flex()
                    .id("browser-storage-rows")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    }

    fn places(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let places = [
            (Place::Cookies, "Cookies".to_owned()),
            (Place::Local, "Local Storage".to_owned()),
            (Place::Session, "Session Storage".to_owned()),
        ];
        let stores = self.storage.databases.iter().flat_map(|database| {
            database.stores.iter().map(|store| {
                let place = Place::Store {
                    database: database.name.clone(),
                    store: store.clone(),
                };
                (place, format!("{} › {store}", database.name))
            })
        });
        let stores: Vec<AnyElement> = stores
            .enumerate()
            .map(|(at, (place, title))| self.place_button(at + places.len(), place, title, cx))
            .collect();
        let places = (places.into_iter().enumerate())
            .map(|(at, (place, title))| self.place_button(at, place, title, cx));
        v_flex()
            .id("browser-storage-places")
            .test_support()
            .flex_none()
            .w(px(PLACES_WIDTH))
            .h_full()
            .overflow_y_scroll()
            .p_1()
            .gap_0p5()
            .border_r_1()
            .border_color(cx.theme().border)
            .children(places)
            .when(!stores.is_empty(), |list| {
                list.child(
                    div()
                        .px_2()
                        .pt_2()
                        .text_color(cx.theme().muted_foreground)
                        .child("IndexedDB"),
                )
            })
            .children(stores)
    }

    fn place_button(
        &self,
        at: usize,
        place: Place,
        title: String,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let chosen = place == self.storage.place;
        let hover = theme.list_hover;
        div()
            .id(("browser-storage-place", at))
            .test_support()
            .aria_label(SharedString::from(title.clone()))
            .px_2()
            .py_1()
            .rounded(theme.radius)
            .truncate()
            .cursor_pointer()
            .hover(move |row| row.bg(hover))
            .when(chosen, |row| row.bg(theme.list_active))
            .on_click(cx.listener(move |this, _, _, cx| this.go_to_place(place.clone(), cx)))
            .child(title)
            .into_any_element()
    }

    fn kept_rows(&self, kept: &Kept, cx: &Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme();
        let removable = !matches!(self.storage.place, Place::Store { .. });
        let pairs: Vec<(String, String)> = match kept {
            Kept::Cookies(cookies) => cookies
                .iter()
                .map(|it| (it.name.clone(), cookie_text(it)))
                .collect(),
            Kept::Pairs(pairs) => pairs.clone(),
            Kept::Entries(entries) => entries.entries.clone(),
        };
        if pairs.is_empty() {
            return vec![muted("Nothing kept here.", theme)];
        }
        let mut rows: Vec<AnyElement> = pairs
            .into_iter()
            .map(|(key, value)| kept_row(&key, value, removable, cx))
            .collect();
        if let Kept::Entries(StoreEntries { more: true, .. }) = kept {
            rows.push(muted("Only the first entries are shown.", theme));
        }
        rows
    }
}

async fn read(page: &Page, place: &Place, url: &str) -> anyhow::Result<Kept> {
    Ok(match place {
        Place::Cookies => Kept::Cookies(page.cookies(url).await?),
        Place::Local => Kept::Pairs(page.web_storage(true).await?),
        Place::Session => Kept::Pairs(page.web_storage(false).await?),
        Place::Store { database, store } => {
            Kept::Entries(page.store_entries(database, store).await?)
        }
    })
}

/// The value, then where and how long it is sent.
fn cookie_text(cookie: &Cookie) -> String {
    let mut parts = vec![
        cookie.value.clone(),
        format!("{}{}", cookie.domain, cookie.path),
    ];
    parts.push(if cookie.expires.is_some() {
        "Expires".into()
    } else {
        "Session".into()
    });
    parts.extend(cookie.http_only.then(|| "HttpOnly".to_owned()));
    parts.extend(cookie.secure.then(|| "Secure".to_owned()));
    parts.extend(cookie.same_site.clone());
    parts.join(" · ")
}

fn kept_row(key: &str, value: String, removable: bool, cx: &Context<Browser>) -> AnyElement {
    let theme = cx.theme();
    let removed = key.to_owned();
    h_flex()
        .id(SharedString::from(format!("browser-kept-{key}")))
        .test_support()
        .aria_label(SharedString::from(key.to_owned()))
        .items_start()
        .gap_2()
        .px_2()
        .py_0p5()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex_none()
                .w(px(KEY_WIDTH))
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .child(key.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(theme.mono_font_family.clone())
                .child(value),
        )
        .when(removable, |row| {
            row.child(
                Button::new(SharedString::from(format!("browser-kept-remove-{key}")))
                    .ghost()
                    .row_icon(IconName::Close)
                    .tooltip("Remove")
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.remove_kept(removed.clone(), cx)),
                    ),
            )
        })
        .into_any_element()
}

fn muted(text: &str, theme: &Theme) -> AnyElement {
    div()
        .p_2()
        .text_color(theme.muted_foreground)
        .child(text.to_owned())
        .into_any_element()
}
