//! USB devices, as `WebUSB`'s `USBDevice`: listed with their descriptors,
//! opened on request, their transfers carried out without a thread each.

mod open;
mod results;

use nusb::{DeviceInfo, MaybeFuture as _, descriptors::TransferType};
use serde_json::{Value, json};

use crate::{DomError, Done, Found, Opened, wants};

const PREFIX: &str = "usb:";

#[derive(Default)]
pub(crate) struct Usb {
    opened: Opened<open::UsbOpen>,
}

/// Vortex's name for a device, which stays the same when it is plugged in
/// again: by its serial number, or by the port it is in when it has none.
fn key(info: &DeviceInfo) -> String {
    let ids = format!("{PREFIX}{:04x}:{:04x}", info.vendor_id(), info.product_id());
    let serial = info.serial_number().map(str::trim);
    if let Some(serial) = serial.filter(|it| !it.is_empty()) {
        return format!("{ids}:{serial}");
    }
    let ports: Vec<String> = info.port_chain().iter().map(u8::to_string).collect();
    format!("{ids}@{}-{}", info.bus_id(), ports.join("."))
}

fn plugged_in() -> Vec<DeviceInfo> {
    nusb::list_devices()
        .wait()
        .map_or_default(Iterator::collect)
}

pub(crate) fn find(filters: &[Value]) -> Vec<Found> {
    plugged_in()
        .iter()
        .filter(|info| filters.is_empty() || filters.iter().any(|filter| matches(filter, info)))
        .map(found)
        .collect()
}

pub(crate) fn present() -> Vec<String> {
    plugged_in().iter().map(key).collect()
}

/// The device Vortex calls `name`, while it is plugged in.
fn info(name: &str) -> Result<DeviceInfo, DomError> {
    plugged_in()
        .into_iter()
        .find(|info| key(info) == name)
        .ok_or_else(DomError::not_found)
}

fn found(info: &DeviceInfo) -> Found {
    let product = info
        .product_string()
        .map(str::trim)
        .filter(|it| !it.is_empty());
    let maker = info
        .manufacturer_string()
        .map(str::trim)
        .filter(|it| !it.is_empty());
    let ids = format!("{:04x}:{:04x}", info.vendor_id(), info.product_id());
    let name = match (product, maker) {
        (Some(product), Some(maker)) => format!("{product} ({maker})"),
        (Some(product), None) => product.to_owned(),
        (None, _) => format!("USB device {ids}"),
    };
    Found {
        id: key(info),
        name,
        info: describe(info),
    }
}

impl Usb {
    pub(crate) async fn call(&self, device: &str, method: &str, args: &Value) -> Done {
        match method {
            "open" => self.open(device).await,
            "close" => {
                self.opened.remove(device);
                Ok(Value::Null)
            }
            _ => self.opened.get(device)?.call(method, args).await,
        }
    }

    async fn open(&self, device: &str) -> Done {
        if let Ok(open) = self.opened.get(device) {
            return Ok(open.state());
        }
        let info = info(device)?;
        let opened = info.open().await;
        let opened = opened
            .map_err(|error| DomError::new("SecurityError", format!("Access denied: {error}")))?;
        let open = open::UsbOpen::new(opened);
        let state = open.state();
        self.opened.insert(device, open);
        Ok(state)
    }

    pub(crate) fn close_all(&self) {
        self.opened.take_all();
    }
}

/// A filter's class matches the device's or one of its interfaces'.
fn matches(filter: &Value, info: &DeviceInfo) -> bool {
    let serial = filter["serialNumber"].as_str();
    let classes = std::iter::once((info.class(), info.subclass(), info.protocol())).chain(
        info.interfaces()
            .map(|it| (it.class(), it.subclass(), it.protocol())),
    );
    let class_matches = classes.into_iter().any(|(class, subclass, protocol)| {
        wants(filter, "classCode", class.into())
            && wants(filter, "subclassCode", subclass.into())
            && wants(filter, "protocolCode", protocol.into())
    });
    wants(filter, "vendorId", info.vendor_id().into())
        && wants(filter, "productId", info.product_id().into())
        && serial.is_none_or(|serial| info.serial_number() == Some(serial))
        && class_matches
}

/// A version in binary-coded decimal: major, minor and subminor.
fn version(bcd: u16) -> [u16; 3] {
    [
        (bcd >> 12 & 0xF) * 10 + (bcd >> 8 & 0xF),
        bcd >> 4 & 0xF,
        bcd & 0xF,
    ]
}

/// The `USBDevice` fields the page reads before opening it. The
/// configurations are read by opening it a moment, as the browser does.
fn describe(info: &DeviceInfo) -> Value {
    let [usb_major, usb_minor, usb_subminor] = version(info.usb_version());
    let [major, minor, subminor] = version(info.device_version());
    let opened = info.open().wait().ok();
    let configurations: Vec<Value> = opened.as_ref().map_or_default(|device| {
        device
            .configurations()
            .map(|it| configuration(&it))
            .collect()
    });
    let active = opened
        .as_ref()
        .and_then(|device| device.active_configuration().ok());
    json!({
        "usbVersionMajor": usb_major,
        "usbVersionMinor": usb_minor,
        "usbVersionSubminor": usb_subminor,
        "deviceClass": info.class(),
        "deviceSubclass": info.subclass(),
        "deviceProtocol": info.protocol(),
        "vendorId": info.vendor_id(),
        "productId": info.product_id(),
        "deviceVersionMajor": major,
        "deviceVersionMinor": minor,
        "deviceVersionSubminor": subminor,
        "manufacturerName": info.manufacturer_string(),
        "productName": info.product_string(),
        "serialNumber": info.serial_number(),
        "configurationValue": active.map(|it| it.configuration_value()),
        "configurations": configurations,
    })
}

fn configuration(configuration: &nusb::descriptors::ConfigurationDescriptor<'_>) -> Value {
    let interfaces: Vec<Value> = configuration
        .interfaces()
        .map(|interface| {
            let alternates: Vec<Value> =
                interface.alt_settings().map(|it| alternate(&it)).collect();
            json!({"interfaceNumber": interface.interface_number(), "alternates": alternates})
        })
        .collect();
    json!({
        "configurationValue": configuration.configuration_value(),
        "configurationName": null,
        "interfaces": interfaces,
    })
}

fn alternate(alternate: &nusb::descriptors::InterfaceDescriptor<'_>) -> Value {
    let endpoints: Vec<Value> = alternate
        .endpoints()
        .map(|endpoint| {
            let kind = match endpoint.transfer_type() {
                TransferType::Bulk => "bulk",
                TransferType::Interrupt => "interrupt",
                TransferType::Isochronous => "isochronous",
                TransferType::Control => "control",
            };
            json!({
                "endpointNumber": endpoint.address() & 0x0F,
                "direction": if endpoint.address() & 0x80 == 0 { "out" } else { "in" },
                "type": kind,
                "packetSize": endpoint.max_packet_size(),
            })
        })
        .collect();
    json!({
        "alternateSetting": alternate.alternate_setting(),
        "interfaceClass": alternate.class(),
        "interfaceSubclass": alternate.subclass(),
        "interfaceProtocol": alternate.protocol(),
        "interfaceName": null,
        "endpoints": endpoints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_binary_coded_decimal() {
        assert_eq!(version(0x0210), [2, 1, 0]);
        assert_eq!(version(0x1234), [12, 3, 4]);
    }
}
