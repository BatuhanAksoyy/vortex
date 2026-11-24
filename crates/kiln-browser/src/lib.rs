//! Web pages drawn by the Chromium-based browser the user has installed. It
//! runs without a window, in a profile of Kiln's own, and is driven over the
//! Chrome `DevTools` Protocol: it sends pictures of the page, and takes the
//! mouse and keyboard. No UI dependencies.

mod address;
mod chrome;
mod console;
mod dialog;
mod download;
mod element;
mod event;
mod find;
mod guest;
mod input;
mod keys;
mod page;
mod page_requests;
mod profile;
mod transport;

pub use address::normalize;
pub use chrome::Chrome;
pub use console::{ConsoleEntry, Level};
pub use dialog::{Dialog, DialogKind};
pub use download::{DownloadProgress, DownloadState, downloads_dir};
pub use element::{Element, Screenshot};
pub use event::{Frame, PageEvent};
pub use find::{Installed, find};
pub use guest::{Choice, MenuTarget, Select};
pub use input::{Button, Modifiers, MouseInput, MouseKind, Wheel};
pub use keys::Key;
pub use page::{History, Page, Viewport};
pub use profile::Profile;
