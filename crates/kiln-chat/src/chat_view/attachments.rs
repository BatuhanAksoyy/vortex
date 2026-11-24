//! What goes with the next message: pictures and files, attached by
//! pasting, by dropping them on the chat or by picking them. They show as
//! cards above the message box until the message is sent.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    ClipboardEntry, ClipboardItem, Context, PathPromptOptions, SharedString, TestSupportExt,
    Window,
    component::{
        Icon, IconName, Sizable,
        attachment::{Attachment as Card, AttachmentContent, AttachmentMedia, AttachmentTitle},
        button::{Button, ButtonVariants},
        h_flex,
    },
    div,
};
use kiln_agent::{Message, Picture, SessionCommand};

use super::{ChatEvent, ChatView};
use crate::{pictures, slash, transcript::Status};

/// The largest picture sent as one; a larger one goes as a file, which the
/// agent reads itself.
const MAX_PICTURE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attachment {
    Picture { name: String, picture: Picture },
    File(PathBuf),
}

impl ChatView {
    /// Sends the composer's text with what is attached, or runs the text
    /// when it is a command Kiln answers itself.
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().trim().to_owned();
        if let Some(command) = slash::native(&text) {
            self.composer
                .update(cx, |composer, cx| composer.clean(window, cx));
            self.run_native(command, window, cx);
            return;
        }
        let message = message(text, &self.attachments);
        if self.submit(message, window, cx) {
            self.attachments.clear();
            self.composer
                .update(cx, |composer, cx| composer.clean(window, cx));
        }
    }

    /// Sends `message` as the next one; `false` if it cannot be sent now
    /// (empty, a turn is running, a usage limit is reached, or the agent
    /// failed to start).
    pub fn submit(
        &mut self,
        message: Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if message.is_empty() || self.status() == Status::Running || self.is_limited() {
            return false;
        }
        let Some(commands) = self.ensure_session(window, cx) else {
            return false;
        };
        let change = self.transcript.push_user(message.clone());
        self.apply_change(change, cx);
        cx.emit(ChatEvent::TurnStarted);
        commands.try_send(SessionCommand::Send(message)).is_ok()
    }

    /// Attaches `paths`: pictures by their contents, read in the
    /// background, and anything else by name.
    pub fn attach_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let attachments = cx
                .background_spawn(async move { paths.iter().map(|path| load(path)).collect() })
                .await;
            let _ = this.update(cx, |this, cx| this.attach(attachments, cx));
        })
        .detach();
    }

    /// Takes back a withdrawn message's attachments, so it can be sent again.
    pub(super) fn restore_attachments(&mut self, message: Message, cx: &mut Context<Self>) {
        let pictures = message
            .pictures
            .into_iter()
            .map(|picture| Attachment::Picture {
                name: "Pasted image".into(),
                picture,
            });
        let files = message.files.into_iter().map(Attachment::File);
        self.attach(pictures.chain(files).collect(), cx);
    }

    /// A paste of pictures or copied files attaches them; `false` lets the
    /// box paste text as usual.
    pub(super) fn paste(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let mut pictures = Vec::new();
        let mut paths = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::Image(image) => pictures.push(Attachment::Picture {
                    name: "Pasted image".into(),
                    picture: Picture {
                        media_type: image.format.mime_type().to_owned(),
                        data: image.bytes.clone().into(),
                    },
                }),
                ClipboardEntry::ExternalPaths(external) => {
                    paths.extend_from_slice(external.paths());
                }
                ClipboardEntry::String(_) => {}
            }
        }
        if pictures.is_empty() && paths.is_empty() {
            return false;
        }
        self.attach(pictures, cx);
        if !paths.is_empty() {
            self.attach_paths(paths, cx);
        }
        true
    }

    pub(super) fn attach_button(cx: &Context<Self>) -> impl IntoElement + use<> {
        Button::new("chat-attach")
            .ghost()
            .small()
            .icon(Icon::new(gpui_kit::assets::IconName::Paperclip))
            .tooltip("Attach files or pictures")
            .on_click(cx.listener(|_, _, window, cx| Self::pick_files(window, cx)))
    }

    fn pick_files(window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |this, cx| this.attach_paths(paths, cx));
            }
        })
        .detach();
    }

    /// The attached pictures and files above the message box.
    pub(super) fn render_attachments(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        if self.attachments.is_empty() {
            return None;
        }
        let cards = self
            .attachments
            .iter()
            .enumerate()
            .map(|(index, attachment)| Self::card(index, attachment, cx));
        Some(
            h_flex()
                .id("chat-attachments")
                .flex_wrap()
                .gap_2()
                .px_2()
                .pt_2()
                .children(cards),
        )
    }

    fn card(index: usize, attachment: &Attachment, cx: &Context<Self>) -> impl IntoElement + use<> {
        let (title, media) = match attachment {
            Attachment::Picture { name, picture } => {
                let media = pictures::image(picture).map_or_else(
                    || AttachmentMedia::new().child(Icon::new(gpui_kit::assets::IconName::Image)),
                    |image| AttachmentMedia::new().src(image),
                );
                (name.clone(), media)
            }
            Attachment::File(path) => (
                file_name(path),
                AttachmentMedia::new().child(Icon::new(IconName::File)),
            ),
        };
        let picture = match attachment {
            Attachment::Picture { picture, .. } => Some(picture.clone()),
            Attachment::File(_) => None,
        };
        let mut card = Card::new()
            .id(SharedString::from(format!("chat-attachment-card-{index}")))
            .small()
            .tooltip(match attachment {
                Attachment::File(path) => path.display().to_string(),
                Attachment::Picture { name, .. } => name.clone(),
            })
            .media(media)
            .content(AttachmentContent::new().title(AttachmentTitle::new(title.clone())))
            .on_remove(cx.listener(move |this, _, _, cx| {
                // Removing is not a click on the card, which shows the picture.
                cx.stop_propagation();
                if index < this.attachments.len() {
                    this.attachments.remove(index);
                    cx.notify();
                }
            }));
        if let Some(picture) = picture {
            card = card.on_click(cx.listener(move |this, _, window, cx| {
                this.view_picture(picture.clone(), window, cx);
            }));
        }
        div()
            .id(SharedString::from(format!("chat-attachment-{index}")))
            .test_support()
            .aria_label(SharedString::from(title))
            .child(card)
    }

    fn attach(&mut self, attachments: Vec<Attachment>, cx: &mut Context<Self>) {
        for attachment in attachments {
            if !self.attachments.contains(&attachment) {
                self.attachments.push(attachment);
            }
        }
        cx.notify();
    }
}

/// The message `text` and `attachments` make.
fn message(text: String, attachments: &[Attachment]) -> Message {
    let mut message = Message::from(text);
    for attachment in attachments {
        match attachment {
            Attachment::Picture { picture, .. } => message.pictures.push(picture.clone()),
            Attachment::File(path) => message.files.push(path.clone()),
        }
    }
    message
}

/// A picture the model can see, by its extension, unless it is too large
/// to send; anything else is named for the agent to read.
fn load(path: &Path) -> Attachment {
    let media_type = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(|extension| match extension.to_ascii_lowercase().as_str() {
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "gif" => Some("image/gif"),
            "webp" => Some("image/webp"),
            _ => None,
        });
    let small = std::fs::metadata(path).is_ok_and(|meta| meta.len() <= MAX_PICTURE_BYTES);
    let data = media_type
        .filter(|_| small)
        .and_then(|_| std::fs::read(path).ok());
    match (media_type, data) {
        (Some(media_type), Some(data)) => Attachment::Picture {
            name: file_name(path),
            picture: Picture {
                media_type: media_type.to_owned(),
                data: data.into(),
            },
        },
        _ => Attachment::File(path.to_owned()),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_pictures_go_by_contents_and_other_files_by_name() {
        let dir = std::env::temp_dir().join(format!("kiln-attach-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let picture = dir.join("shot.PNG");
        let note = dir.join("note.txt");
        let _ = std::fs::write(&picture, b"\x89PNG");
        let _ = std::fs::write(&note, b"hi");
        assert!(matches!(
            load(&picture),
            Attachment::Picture { name, picture } if name == "shot.PNG" && picture.media_type == "image/png"
        ));
        assert_eq!(load(&note), Attachment::File(note.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
