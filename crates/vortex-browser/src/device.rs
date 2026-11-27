//! The browser asking which device a page may use, as Web Bluetooth's
//! `requestDevice` does. Its own chooser would not show without a window.

use serde_json::{Value, json};

use crate::Page;

/// The devices found so far; asked again, with the same id, as more are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRequest {
    pub id: String,
    pub devices: Vec<Device>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
}

pub(crate) fn parse(params: &Value) -> Option<DeviceRequest> {
    let devices = params["devices"]
        .as_array()?
        .iter()
        .filter_map(|device| {
            Some(Device {
                id: device["id"].as_str()?.to_owned(),
                name: device["name"].as_str().unwrap_or_default().to_owned(),
            })
        })
        .collect();
    Some(DeviceRequest {
        id: params["id"].as_str()?.to_owned(),
        devices,
    })
}

impl Page {
    /// Gives the page the device it asked for in `request`.
    pub fn pick_device(&self, request: &str, device: &str) {
        let pick = json!({"id": request, "deviceId": device});
        self.send("DeviceAccess.selectPrompt", &pick);
    }

    /// Tells the page the user chose no device.
    pub fn refuse_device(&self, request: &str) {
        self.send("DeviceAccess.cancelPrompt", &json!({"id": request}));
    }
}

#[cfg(test)]
mod installed;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_lists_the_devices_found() {
        let params =
            json!({"id": "7", "devices": [{"id": "a", "name": "Heart Rate"}, {"id": "b"}]});
        let request = parse(&params).unwrap();
        assert_eq!(request.id, "7");
        let names: Vec<_> = request.devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Heart Rate", ""]);
    }
}
