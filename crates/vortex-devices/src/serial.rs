//! Serial ports, as Web Serial's `SerialPort`: opened with the page's
//! settings, read on a thread of their own, written and signalled on call.

use std::{
    io::{ErrorKind, Read as _, Write as _},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_channel::Sender;
use serde_json::{Value, json};
use serialport::{
    DataBits, FlowControl, Parity, SerialPort, SerialPortInfo, SerialPortType, StopBits,
};

use crate::{DeviceEvent, DomError, Done, Found, Kind, Opened, bytes, encode, number, send, wants};

/// How long a read waits before it looks whether the port was closed.
const READ_WAIT: Duration = Duration::from_millis(100);
/// Web Serial's default.
const BUFFER: usize = 255;
const PREFIX: &str = "serial:";

pub(crate) struct Serial {
    opened: Opened<Port>,
    events: Sender<DeviceEvent>,
}

struct Port {
    port: Mutex<Box<dyn SerialPort>>,
    closed: Arc<AtomicBool>,
}

impl Drop for Port {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

/// The ports plugged in. macOS lists each twice; the call-out one is what
/// is used.
fn listed() -> Vec<SerialPortInfo> {
    let ports = serialport::available_ports().unwrap_or_default();
    let names: Vec<String> = ports.iter().map(|port| port.port_name.clone()).collect();
    ports
        .into_iter()
        .filter(|port| {
            let twin = port.port_name.replacen("/dev/tty.", "/dev/cu.", 1);
            twin == port.port_name || !names.contains(&twin)
        })
        .collect()
}

/// Vortex's name for a port, which stays the same when its USB device is
/// plugged in again, wherever: by the device's serial number and the
/// port's interface, or else by the port's name.
fn key(port: &SerialPortInfo) -> String {
    if let SerialPortType::UsbPort(usb) = &port.port_type
        && let Some(serial) = usb
            .serial_number
            .as_deref()
            .filter(|it| !it.trim().is_empty())
    {
        let interface = usb.interface.map_or_default(|it| it.to_string());
        return format!(
            "{PREFIX}{:04x}:{:04x}:{}:{interface}",
            usb.vid,
            usb.pid,
            serial.trim()
        );
    }
    format!("{PREFIX}{}", port.port_name)
}

pub(crate) fn present() -> Vec<String> {
    listed().iter().map(key).collect()
}

pub(crate) fn find(filters: &[Value]) -> Vec<Found> {
    listed()
        .iter()
        .map(found)
        .filter(|found| {
            filters.is_empty() || filters.iter().any(|filter| matches(filter, &found.info))
        })
        .collect()
}

fn found(port: &SerialPortInfo) -> Found {
    let (info, name) = match &port.port_type {
        SerialPortType::UsbPort(usb) => (
            json!({"usbVendorId": usb.vid, "usbProductId": usb.pid}),
            usb.product.clone(),
        ),
        _ => (json!({}), None),
    };
    let short = port.port_name.rsplit('/').next().unwrap_or(&port.port_name);
    let name = name.map_or_else(
        || short.to_owned(),
        |product| format!("{product} ({short})"),
    );
    Found {
        id: key(port),
        name,
        info,
    }
}

/// A port without USB ids matches no filter, as in the browser.
fn matches(filter: &Value, info: &Value) -> bool {
    let Some(vendor) = info["usbVendorId"].as_u64() else {
        return false;
    };
    let product = info["usbProductId"].as_u64().unwrap_or_default();
    wants(filter, "usbVendorId", vendor) && wants(filter, "usbProductId", product)
}

impl Serial {
    pub(crate) fn new(events: Sender<DeviceEvent>) -> Self {
        Self {
            opened: Opened::default(),
            events,
        }
    }

    pub(crate) fn call(&self, device: &str, method: &str, args: &Value) -> Done {
        match method {
            "open" => self.open(device, args),
            "close" => {
                self.opened.remove(device);
                Ok(Value::Null)
            }
            "write" => self.write(device, &bytes(args, "data")?),
            "setSignals" => self.set_signals(device, args),
            "getSignals" => self.signals(device),
            _ => Err(DomError::unsupported(method)),
        }
    }

    pub(crate) fn close_all(&self) {
        self.opened.take_all();
    }

    fn open(&self, device: &str, args: &Value) -> Done {
        if self.opened.has(device) {
            return Err(DomError::new(
                "InvalidStateError",
                "The port is already open.",
            ));
        }
        let settings = settings(args)?;
        let port = listed().into_iter().find(|port| key(port) == device);
        let path = port.ok_or_else(DomError::not_found)?.port_name;
        let port = settings.path(path).open().map_err(|error| {
            DomError::new(
                "NetworkError",
                format!("Failed to open serial port: {error}"),
            )
        })?;
        let size = args["bufferSize"]
            .as_u64()
            .map_or(BUFFER, |size| usize::try_from(size).unwrap_or(BUFFER));
        self.start(device, port, size.max(1))
    }

    /// Reads `port` from now on, and lets the page write to it.
    fn start(&self, device: &str, port: Box<dyn SerialPort>, size: usize) -> Done {
        let reader = port
            .try_clone()
            .map_err(|_| DomError::new("NetworkError", "Failed to open serial port."))?;
        let closed = Arc::new(AtomicBool::new(false));
        let reading = Reading {
            port: reader,
            closed: closed.clone(),
            events: self.events.clone(),
            device: device.to_owned(),
        };
        std::thread::spawn(move || reading.run(size));
        let port = Port {
            port: Mutex::new(port),
            closed,
        };
        self.opened.insert(device, port);
        Ok(Value::Null)
    }

    fn write(&self, device: &str, data: &[u8]) -> Done {
        let port = self.opened.get(device)?;
        let mut port = port.port.lock().unwrap_or_else(PoisonError::into_inner);
        let written = port.write_all(data).and_then(|()| port.flush());
        written.map_err(|_| DomError::new("NetworkError", "The device has been lost."))?;
        Ok(Value::Null)
    }

    fn set_signals(&self, device: &str, args: &Value) -> Done {
        let port = self.opened.get(device)?;
        let mut port = port.port.lock().unwrap_or_else(PoisonError::into_inner);
        let failed = |_| DomError::new("NetworkError", "Failed to set control signals.");
        if let Some(ready) = args["dataTerminalReady"].as_bool() {
            port.write_data_terminal_ready(ready).map_err(failed)?;
        }
        if let Some(request) = args["requestToSend"].as_bool() {
            port.write_request_to_send(request).map_err(failed)?;
        }
        match args["break"].as_bool() {
            Some(true) => port.set_break().map_err(failed)?,
            Some(false) => port.clear_break().map_err(failed)?,
            None => {}
        }
        Ok(Value::Null)
    }

    fn signals(&self, device: &str) -> Done {
        let port = self.opened.get(device)?;
        let mut port = port.port.lock().unwrap_or_else(PoisonError::into_inner);
        let failed = |_| DomError::new("NetworkError", "Failed to get control signals.");
        Ok(json!({
            "dataCarrierDetect": port.read_carrier_detect().map_err(failed)?,
            "clearToSend": port.read_clear_to_send().map_err(failed)?,
            "ringIndicator": port.read_ring_indicator().map_err(failed)?,
            "dataSetReady": port.read_data_set_ready().map_err(failed)?,
        }))
    }
}

fn settings(args: &Value) -> Result<serialport::SerialPortBuilder, DomError> {
    let baud: u32 = number(args, "baudRate")?;
    if baud == 0 {
        return Err(DomError::argument("baudRate"));
    }
    let (data_bits, stop_bits) = framing(args)?;
    let (parity, flow) = checking(args)?;
    Ok(serialport::new("", baud)
        .data_bits(data_bits)
        .stop_bits(stop_bits)
        .parity(parity)
        .flow_control(flow)
        .timeout(READ_WAIT))
}

fn framing(args: &Value) -> Result<(DataBits, StopBits), DomError> {
    let data_bits = match args["dataBits"].as_u64() {
        None | Some(8) => DataBits::Eight,
        Some(7) => DataBits::Seven,
        _ => return Err(DomError::argument("dataBits")),
    };
    let stop_bits = match args["stopBits"].as_u64() {
        None | Some(1) => StopBits::One,
        Some(2) => StopBits::Two,
        _ => return Err(DomError::argument("stopBits")),
    };
    Ok((data_bits, stop_bits))
}

fn checking(args: &Value) -> Result<(Parity, FlowControl), DomError> {
    let parity = match args["parity"].as_str() {
        None | Some("none") => Parity::None,
        Some("even") => Parity::Even,
        Some("odd") => Parity::Odd,
        _ => return Err(DomError::argument("parity")),
    };
    let flow = match args["flowControl"].as_str() {
        None | Some("none") => FlowControl::None,
        Some("hardware") => FlowControl::Hardware,
        _ => return Err(DomError::argument("flowControl")),
    };
    Ok((parity, flow))
}

/// Reads what arrives until the port is closed or lost.
struct Reading {
    port: Box<dyn SerialPort>,
    closed: Arc<AtomicBool>,
    events: Sender<DeviceEvent>,
    device: String,
}

impl Reading {
    /// Unless it was closed on purpose.
    fn lost(&self) {
        if !self.closed.load(Ordering::Relaxed) {
            send(
                &self.events,
                Kind::Serial,
                &self.device,
                json!({"type": "lost"}),
            );
        }
    }

    fn run(mut self, size: usize) {
        let mut buffer = vec![0; size];
        while !self.closed.load(Ordering::Relaxed) {
            match self.port.read(&mut buffer) {
                Ok(0) => {}
                Ok(read) => {
                    let data = json!({"type": "data", "data": encode(&buffer[..read])});
                    send(&self.events, Kind::Serial, &self.device, data);
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
                Err(_) => {
                    self.lost();
                    return;
                }
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
