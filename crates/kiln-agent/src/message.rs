//! What the user sends, and the pictures that travel in messages and tool
//! output.

use std::{path::PathBuf, sync::Arc};

/// A picture in a message or in a tool's output, as its file holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    /// E.g. `image/png`.
    pub media_type: String,
    pub data: Arc<[u8]>,
}

/// A message to the agent: text, with pictures and files attached.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    /// Shown to the model as they are.
    pub pictures: Vec<Picture>,
    /// Named for the agent to read; their contents are not sent.
    pub files: Vec<PathBuf>,
}

impl Message {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.pictures.is_empty() && self.files.is_empty()
    }
}

impl From<String> for Message {
    fn from(text: String) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }
}

impl From<&str> for Message {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}
