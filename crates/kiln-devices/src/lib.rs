//! USB, HID and serial devices, for pages that use `WebUSB`, `WebHID` and
//! Web Serial. The browser Kiln drives shows no chooser for them and reaches no
//! device, so Kiln does: it lists the devices, opens the one the user
//! picked, and carries out each call the page makes, in the shape the Web
//! API gives it.

mod hid;
mod serial;
mod usb;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use async_channel::{Receiver, Sender};
use base64::Engine as _;
use serde_json::Value;

/// Which of the three APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Usb,
    Hid,
    Serial,
}

impl Kind {
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "usb" => Some(Self::Usb),
            "hid" => Some(Self::Hid),
            "serial" => Some(Self::Serial),
            _ => None,
        }
    }

    /// The API's name in the page: `usb`, `hid` or `serial`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Usb => "usb",
            Self::Hid => "hid",
            Self::Serial => "serial",
        }
    }

    /// What the page asks for, in a chooser's words.
    #[must_use]
    pub fn noun(self) -> &'static str {
        match self {
            Self::Usb => "a USB device",
            Self::Hid => "an HID device",
            Self::Serial => "a serial port",
        }
    }
}

/// A `DOMException`: its name and message, as the page sees them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomError {
    pub name: &'static str,
    pub message: String,
}

impl DomError {
    #[must_use]
    pub fn new(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            message: message.into(),
        }
    }

    fn not_found() -> Self {
        Self::new("NotFoundError", "The device was disconnected.")
    }

    fn not_open() -> Self {
        Self::new("InvalidStateError", "The device must be opened first.")
    }

    fn unsupported(what: &str) -> Self {
        Self::new("NotSupportedError", format!("{what} is not supported."))
    }

    fn argument(what: &str) -> Self {
        Self::new("TypeError", format!("Invalid {what}."))
    }
}

pub type Done = Result<Value, DomError>;

/// A device the page could be given.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// Kiln's name for it, the same while it stays plugged in.
    pub id: String,
    /// As the chooser lists it.
    pub name: String,
    /// What the page reads of it: a `USBDevice`'s, `HIDDevice`'s or
    /// `SerialPort`'s fields.
    pub info: Value,
}

/// Something a device did by itself, for the page.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceEvent {
    pub kind: Kind,
    pub device: String,
    /// `{"type": "data", …}`, `{"type": "inputreport", …}` or
    /// `{"type": "lost"}`.
    pub event: Value,
}

/// The devices one page has open, and the calls it makes on them.
pub struct Devices {
    usb: usb::Usb,
    hid: hid::Hid,
    serial: serial::Serial,
}

impl Devices {
    /// With what the open devices report by themselves.
    #[must_use]
    pub fn new() -> (Self, Receiver<DeviceEvent>) {
        let (events, received) = async_channel::unbounded();
        let devices = Self {
            usb: usb::Usb::default(),
            hid: hid::Hid::new(events.clone()),
            serial: serial::Serial::new(events),
        };
        (devices, received)
    }

    /// The devices plugged in that match the page's `filters`, an array of
    /// the API's filter dictionaries; all of them when it is empty or
    /// missing. Reading them can take a moment, so not on the UI thread.
    #[must_use]
    pub fn find(&self, kind: Kind, filters: &Value) -> Vec<Found> {
        let filters = filters.as_array().map_or_default(Vec::as_slice);
        match kind {
            Kind::Usb => usb::find(filters),
            Kind::Hid => hid::find(filters),
            Kind::Serial => serial::find(filters),
        }
    }

    /// The ids of the devices plugged in, read quickly enough to be asked
    /// every few seconds; USB devices not yet found have none.
    #[must_use]
    pub fn present(&self, kind: Kind) -> Vec<String> {
        match kind {
            Kind::Usb => usb::present(),
            Kind::Hid => hid::present(),
            Kind::Serial => serial::present(),
        }
    }

    /// Carries out `method` on `device` with `args`, as its API would. It
    /// can wait on the device for as long as the page asked.
    pub async fn call(&self, kind: Kind, device: &str, method: &str, args: &Value) -> Done {
        match kind {
            Kind::Usb => self.usb.call(device, method, args).await,
            Kind::Hid => self.hid.call(device, method, args),
            Kind::Serial => self.serial.call(device, method, args),
        }
    }

    /// Lets go of every device, as when the page goes away.
    pub fn close_all(&self) {
        self.usb.close_all();
        self.hid.close_all();
        self.serial.close_all();
    }
}

/// The open handles of one kind, by device id.
struct Opened<T>(Mutex<HashMap<String, Arc<T>>>);

impl<T> Default for Opened<T> {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

impl<T> Opened<T> {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<T>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn get(&self, device: &str) -> Result<Arc<T>, DomError> {
        self.lock()
            .get(device)
            .cloned()
            .ok_or_else(DomError::not_open)
    }

    fn has(&self, device: &str) -> bool {
        self.lock().contains_key(device)
    }

    fn insert(&self, device: &str, handle: T) {
        self.lock().insert(device.to_owned(), Arc::new(handle));
    }

    fn remove(&self, device: &str) -> Option<Arc<T>> {
        self.lock().remove(device)
    }

    fn take_all(&self) -> Vec<Arc<T>> {
        self.lock().drain().map(|(_, handle)| handle).collect()
    }
}

fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The bytes the page sent, as base64 under `key`.
fn bytes(args: &Value, key: &str) -> Result<Vec<u8>, DomError> {
    let text = args[key].as_str().ok_or_else(|| DomError::argument(key))?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(text);
    decoded.map_err(|_| DomError::argument(key))
}

/// A whole number of the page's that must fit `T`.
fn number<T: TryFrom<u64>>(args: &Value, key: &str) -> Result<T, DomError> {
    let value = args[key].as_u64().ok_or_else(|| DomError::argument(key))?;
    T::try_from(value).map_err(|_| DomError::argument(key))
}

/// A filter's field, which is left out when it does not count.
fn wants(filter: &Value, key: &str, value: u64) -> bool {
    filter[key].as_u64().is_none_or(|wanted| wanted == value)
}

fn send(events: &Sender<DeviceEvent>, kind: Kind, device: &str, event: Value) {
    let event = DeviceEvent {
        kind,
        device: device.to_owned(),
        event,
    };
    // Nobody listens once the page is gone.
    let _ = events.try_send(event);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_filter_counts_only_what_it_names() {
        let filter = json!({"vendorId": 0x2341});
        assert!(wants(&filter, "vendorId", 0x2341));
        assert!(!wants(&filter, "vendorId", 0x1234));
        assert!(wants(&filter, "productId", 7));
    }

    #[test]
    fn the_page_sends_bytes_as_base64() {
        let args = json!({"data": "AQID", "length": 300});
        assert_eq!(bytes(&args, "data"), Ok(vec![1, 2, 3]));
        assert_eq!(encode(&[1, 2, 3]), "AQID");
        assert!(number::<u8>(&args, "length").is_err());
        assert_eq!(number::<u16>(&args, "length"), Ok(300));
    }
}
