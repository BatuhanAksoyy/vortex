//! Web pages drawn by the Chromium-based browser the user has installed. It
//! runs without a window, in a profile of Kiln's own, and is driven over the
//! Chrome `DevTools` Protocol: it sends pictures of the page, and takes the
//! mouse and keyboard. No UI dependencies.

mod address;
mod chrome;
mod event;
mod find;
mod input;
mod keys;
mod page;
mod transport;

pub use address::normalize;
pub use chrome::Chrome;
pub use event::{Frame, PageEvent};
pub use find::{Installed, find};
pub use input::{Button, Modifiers, MouseInput, MouseKind, Wheel};
pub use keys::Key;
pub use page::{History, Page, Viewport};
