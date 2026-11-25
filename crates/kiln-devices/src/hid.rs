//! HID devices, as `WebHID`'s `HIDDevice`: one per device, whatever number of
//! collections it has; input reports are read on a thread of their own.

mod descriptor;

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use async_channel::Sender;
use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde_json::{Value, json};

use crate::{DeviceEvent, DomError, Done, Found, Kind, Opened, bytes, encode, number, send, wants};

const PREFIX: &str = "hid:";
/// Longer than any report; a report descriptor is at most 4 KiB.
const MAX_REPORT: usize = 4096;
/// How long a read holds the device before a write may have it.
const READ_WAIT_MS: i32 = 20;

pub(crate) struct Hid {
    opened: Opened<Handle>,
    events: Sender<DeviceEvent>,
}

struct Handle {
    device: Arc<Mutex<HidDevice>>,
    closed: Arc<AtomicBool>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

/// The browser keeps keyboards and security keys from pages.
fn protected(usage_page: u16, usage: u16) -> bool {
    usage_page == 0xF1D0 || (usage_page == 1 && matches!(usage, 6 | 7))
}

pub(crate) fn present() -> Vec<String> {
    let Ok(api) = HidApi::new() else {
        return Vec::new();
    };
    let mut ids: Vec<String> = api.device_list().map(key).collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Kiln's name for a device, which stays the same when it is plugged in
/// again: by its serial number and interface, or else by its path, which
/// the system may change.
fn key(info: &DeviceInfo) -> String {
    let serial = info
        .serial_number()
        .map(str::trim)
        .filter(|it| !it.is_empty());
    match serial {
        Some(serial) => format!(
            "{PREFIX}{:04x}:{:04x}:{serial}:{}",
            info.vendor_id(),
            info.product_id(),
            info.interface_number()
        ),
        None => format!("{PREFIX}{}", info.path().to_string_lossy()),
    }
}

pub(crate) fn find(filters: &[Value]) -> Vec<Found> {
    let Ok(api) = HidApi::new() else {
        return Vec::new();
    };
    let mut by_path: BTreeMap<&std::ffi::CStr, Vec<&DeviceInfo>> = BTreeMap::new();
    for info in api.device_list() {
        by_path.entry(info.path()).or_default().push(info);
    }
    by_path
        .into_values()
        .filter(|infos| {
            !infos
                .iter()
                .all(|info| protected(info.usage_page(), info.usage()))
        })
        .filter(|infos| filters.is_empty() || filters.iter().any(|filter| matches(filter, infos)))
        .map(|infos| found(&api, &infos))
        .collect()
}

fn matches(filter: &Value, infos: &[&DeviceInfo]) -> bool {
    infos.iter().any(|info| {
        wants(filter, "vendorId", info.vendor_id().into())
            && wants(filter, "productId", info.product_id().into())
            && wants(filter, "usagePage", info.usage_page().into())
            && wants(filter, "usage", info.usage().into())
    })
}

fn found(api: &HidApi, infos: &[&DeviceInfo]) -> Found {
    let first = infos[0];
    let name = first.product_string().unwrap_or_default().trim().to_owned();
    let collections = read_descriptor(api, first).map_or_else(
        || infos.iter().map(|info| bare_collection(info)).collect(),
        |parsed| parsed.collections,
    );
    let name = if name.is_empty() {
        format!(
            "HID device {:04x}:{:04x}",
            first.vendor_id(),
            first.product_id()
        )
    } else {
        name
    };
    Found {
        id: key(first),
        info: json!({
            "vendorId": first.vendor_id(),
            "productId": first.product_id(),
            "productName": name,
            "collections": collections,
        }),
        name,
    }
}

fn read_descriptor(api: &HidApi, info: &DeviceInfo) -> Option<descriptor::Parsed> {
    let device = info.open_device(api).ok()?;
    let mut buffer = vec![0; MAX_REPORT];
    let read = device.get_report_descriptor(&mut buffer).ok()?;
    let parsed = descriptor::parse(buffer.get(..read)?);
    (!parsed.collections.is_empty()).then_some(parsed)
}

/// What the system says of a collection when its descriptor is unread.
fn bare_collection(info: &DeviceInfo) -> Value {
    json!({
        "usagePage": info.usage_page(),
        "usage": info.usage(),
        "type": 1,
        "children": [],
        "inputReports": [],
        "outputReports": [],
        "featureReports": [],
    })
}

impl Hid {
    pub(crate) fn new(events: Sender<DeviceEvent>) -> Self {
        Self {
            opened: Opened::default(),
            events,
        }
    }

    pub(crate) fn call(&self, device: &str, method: &str, args: &Value) -> Done {
        match method {
            "open" => self.open(device),
            "close" => {
                self.opened.remove(device);
                Ok(Value::Null)
            }
            "sendReport" => self.send_report(device, args, false),
            "sendFeatureReport" => self.send_report(device, args, true),
            "receiveFeatureReport" => self.receive_feature(device, args),
            _ => Err(DomError::unsupported(method)),
        }
    }

    pub(crate) fn close_all(&self) {
        self.opened.take_all();
    }

    fn open(&self, id: &str) -> Done {
        if self.opened.has(id) {
            return Ok(Value::Null);
        }
        let failed = |_| DomError::new("NotAllowedError", "Failed to open the device.");
        let api = HidApi::new().map_err(failed)?;
        let info = api.device_list().find(|info| key(info) == id);
        let device = info
            .ok_or_else(DomError::not_found)?
            .open_device(&api)
            .map_err(failed)?;
        let mut buffer = vec![0; MAX_REPORT];
        let numbered = (device.get_report_descriptor(&mut buffer).ok())
            .and_then(|read| buffer.get(..read))
            .is_some_and(|read| descriptor::parse(read).numbered);
        let device = Arc::new(Mutex::new(device));
        let closed = Arc::new(AtomicBool::new(false));
        let reading = Reading {
            device: device.clone(),
            closed: closed.clone(),
            events: self.events.clone(),
            id: id.to_owned(),
            numbered,
        };
        std::thread::spawn(move || reading.run());
        self.opened.insert(id, Handle { device, closed });
        Ok(Value::Null)
    }

    /// The report goes out after its id, which is 0 when the device does
    /// not number them.
    fn send_report(&self, id: &str, args: &Value, feature: bool) -> Done {
        let handle = self.opened.get(id)?;
        let mut report = vec![number::<u8>(args, "reportId")?];
        report.extend(bytes(args, "data")?);
        let device = handle.device.lock().unwrap_or_else(PoisonError::into_inner);
        let sent = if feature {
            device.send_feature_report(&report)
        } else {
            device.write(&report).map(|_| ())
        };
        sent.map_err(|_| DomError::new("NotAllowedError", "Failed to write the report."))?;
        Ok(Value::Null)
    }

    /// Its bytes, after the report id unless that is 0, as the browser
    /// gives them.
    fn receive_feature(&self, id: &str, args: &Value) -> Done {
        let handle = self.opened.get(id)?;
        let report_id = number::<u8>(args, "reportId")?;
        let mut buffer = vec![0; MAX_REPORT];
        buffer[0] = report_id;
        let device = handle.device.lock().unwrap_or_else(PoisonError::into_inner);
        let read = device.get_feature_report(&mut buffer);
        let read = read.map_err(|_| {
            DomError::new("NotAllowedError", "Failed to receive the feature report.")
        })?;
        let start = usize::from(report_id == 0);
        let data = buffer.get(start..read.max(start)).unwrap_or_default();
        Ok(Value::String(encode(data)))
    }
}

struct Reading {
    device: Arc<Mutex<HidDevice>>,
    closed: Arc<AtomicBool>,
    events: Sender<DeviceEvent>,
    id: String,
    numbered: bool,
}

impl Reading {
    /// Unless it was closed on purpose.
    fn lost(&self) {
        if !self.closed.load(Ordering::Relaxed) {
            send(&self.events, Kind::Hid, &self.id, json!({"type": "lost"}));
        }
    }

    fn run(self) {
        let mut buffer = vec![0; MAX_REPORT];
        while !self.closed.load(Ordering::Relaxed) {
            let read = {
                let device = self.device.lock().unwrap_or_else(PoisonError::into_inner);
                device.read_timeout(&mut buffer, READ_WAIT_MS)
            };
            match read {
                Ok(0) => std::thread::yield_now(),
                Ok(read) => self.report(&buffer[..read]),
                Err(_) => {
                    self.lost();
                    return;
                }
            }
        }
    }

    fn report(&self, read: &[u8]) {
        let (report_id, data) = match read.split_first() {
            Some((id, rest)) if self.numbered => (*id, rest),
            _ => (0, read),
        };
        let event = json!({"type": "inputreport", "reportId": report_id, "data": encode(data)});
        send(&self.events, Kind::Hid, &self.id, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboards_and_security_keys_are_kept_from_pages() {
        assert!(protected(1, 6));
        assert!(protected(0xF1D0, 1));
        assert!(!protected(1, 2));
        assert!(!protected(0xFF00, 1));
    }
}
