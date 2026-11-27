//! An open USB device: its claimed interfaces and their endpoints, and the
//! transfers the page asks for. A transfer waits for the device as long as
//! it takes, as in the browser, without holding a thread.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use futures::lock::Mutex as AsyncMutex;
use nusb::{
    Device, Endpoint, Interface,
    descriptors::TransferType,
    transfer::{Buffer, Bulk, ControlIn, ControlOut, In, Interrupt, Out, TransferError},
};
use serde_json::{Value, json};

use super::results::{in_failure, network, out_result, parse_setup, unclaimed, wrong_direction};
use crate::{DomError, Done, bytes, encode, number};

/// A control transfer the device does not answer fails after this.
const CONTROL_WAIT: Duration = Duration::from_secs(30);
/// Classes the browser keeps from pages: audio, HID, mass storage, smart
/// cards, video, audio/video and wireless controllers.
const PROTECTED: [u8; 7] = [0x01, 0x03, 0x08, 0x0B, 0x0E, 0x10, 0xE0];

enum Pipe {
    BulkIn(Endpoint<Bulk, In>),
    BulkOut(Endpoint<Bulk, Out>),
    InterruptIn(Endpoint<Interrupt, In>),
    InterruptOut(Endpoint<Interrupt, Out>),
}

type Pipes = HashMap<u8, (u8, Arc<AsyncMutex<Pipe>>)>;

pub(crate) struct UsbOpen {
    device: Device,
    interfaces: Mutex<HashMap<u8, Interface>>,
    /// By endpoint address, with the interface each belongs to.
    pipes: Mutex<Pipes>,
}

impl UsbOpen {
    pub(crate) fn new(device: Device) -> Self {
        Self {
            device,
            interfaces: Mutex::new(HashMap::new()),
            pipes: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn state(&self) -> Value {
        let active = self.device.active_configuration().ok();
        json!({"configurationValue": active.map(|it| it.configuration_value())})
    }

    pub(crate) async fn call(&self, method: &str, args: &Value) -> Done {
        match method {
            "selectConfiguration" => {
                self.select_configuration(number(args, "configurationValue")?)
                    .await
            }
            "claimInterface" => self.claim(number(args, "interfaceNumber")?).await,
            "releaseInterface" => Ok(self.release(number(args, "interfaceNumber")?)),
            "selectAlternateInterface" => {
                let interface = number(args, "interfaceNumber")?;
                self.select_alternate(interface, number(args, "alternateSetting")?)
                    .await
            }
            "clearHalt" => self.clear_halt(args).await,
            "reset" => self
                .device
                .reset()
                .await
                .map(|()| Value::Null)
                .map_err(|error| network(&error)),
            _ => self.transfer(method, args).await,
        }
    }

    async fn transfer(&self, method: &str, args: &Value) -> Done {
        match method {
            "controlTransferIn" => {
                self.control_in(&args["setup"], number(args, "length")?)
                    .await
            }
            "controlTransferOut" => {
                self.control_out(&args["setup"], &bytes(args, "data")?)
                    .await
            }
            "transferIn" => {
                self.transfer_in(number(args, "endpointNumber")?, number(args, "length")?)
                    .await
            }
            "transferOut" => {
                self.transfer_out(number(args, "endpointNumber")?, bytes(args, "data")?)
                    .await
            }
            _ => Err(DomError::unsupported(method)),
        }
    }

    fn interface(&self, number: u8) -> Option<Interface> {
        let interfaces = self
            .interfaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        interfaces.get(&number).cloned()
    }

    async fn select_configuration(&self, value: u8) -> Done {
        if self
            .device
            .active_configuration()
            .is_ok_and(|it| it.configuration_value() == value)
        {
            return Ok(self.state());
        }
        self.pipes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.interfaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.device
            .set_configuration(value)
            .await
            .map_err(|error| network(&error))?;
        Ok(self.state())
    }

    async fn claim(&self, number: u8) -> Done {
        if self.interface(number).is_some() {
            return Ok(Value::Null);
        }
        let configuration = self
            .device
            .active_configuration()
            .map_err(|_| DomError::not_open())?;
        let mut alternates = configuration
            .interface_alt_settings()
            .filter(|it| it.interface_number() == number);
        let first = alternates
            .next()
            .ok_or_else(|| DomError::new("NotFoundError", "The interface was not found."))?;
        if PROTECTED.contains(&first.class()) {
            let message = "The requested interface implements a protected class.";
            return Err(DomError::new("SecurityError", message));
        }
        let interface = self.device.detach_and_claim_interface(number).await;
        let interface =
            interface.map_err(|_| DomError::new("NetworkError", "Unable to claim interface."))?;
        self.interfaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(number, interface);
        Ok(Value::Null)
    }

    fn release(&self, number: u8) -> Value {
        self.drop_pipes(number);
        self.interfaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&number);
        Value::Null
    }

    fn drop_pipes(&self, interface: u8) {
        let mut pipes = self.pipes.lock().unwrap_or_else(PoisonError::into_inner);
        pipes.retain(|_, (owner, _)| *owner != interface);
    }

    async fn select_alternate(&self, number: u8, alternate: u8) -> Done {
        let interface = self.interface(number).ok_or_else(unclaimed)?;
        self.drop_pipes(number);
        interface
            .set_alt_setting(alternate)
            .await
            .map_err(|error| network(&error))?;
        Ok(Value::Null)
    }

    async fn control_in(&self, setup: &Value, length: u16) -> Done {
        let (control_type, recipient, request, value, index) = parse_setup(setup)?;
        let data = ControlIn {
            control_type,
            recipient,
            request,
            value,
            index,
            length,
        };
        let read = match self.claimed_any() {
            Some(interface) => interface.control_in(data, CONTROL_WAIT).await,
            None => device_control_in(&self.device, data).await,
        };
        Ok(match read {
            Ok(read) => json!({"status": "ok", "data": encode(&read)}),
            Err(error) => in_failure(error)?,
        })
    }

    async fn control_out(&self, setup: &Value, data: &[u8]) -> Done {
        let (control_type, recipient, request, value, index) = parse_setup(setup)?;
        let out = ControlOut {
            control_type,
            recipient,
            request,
            value,
            index,
            data,
        };
        let sent = match self.claimed_any() {
            Some(interface) => interface.control_out(out, CONTROL_WAIT).await,
            None => device_control_out(&self.device, out).await,
        };
        out_result(sent.map(|()| data.len()))
    }

    /// Control transfers to a claimed interface work on every system.
    fn claimed_any(&self) -> Option<Interface> {
        let interfaces = self
            .interfaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        interfaces.values().next().cloned()
    }

    /// The endpoint at `address` of a claimed interface, opened once.
    fn pipe(&self, address: u8) -> Result<Arc<AsyncMutex<Pipe>>, DomError> {
        let mut pipes = self.pipes.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, pipe)) = pipes.get(&address) {
            return Ok(pipe.clone());
        }
        let interfaces = self
            .interfaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let found = interfaces.iter().find_map(|(number, interface)| {
            let descriptor = interface.descriptor()?;
            let endpoint = descriptor.endpoints().find(|it| it.address() == address)?;
            Some((*number, interface.clone(), endpoint.transfer_type()))
        });
        let (owner, interface, kind) = found.ok_or_else(|| {
            DomError::new(
                "NotFoundError",
                "The specified endpoint is not part of a claimed and selected alternate interface.",
            )
        })?;
        let pipe = open_pipe(&interface, address, kind)?;
        let pipe = Arc::new(AsyncMutex::new(pipe));
        pipes.insert(address, (owner, pipe.clone()));
        Ok(pipe)
    }

    async fn transfer_in(&self, endpoint: u8, length: usize) -> Done {
        let pipe = self.pipe(endpoint | 0x80)?;
        let mut pipe = pipe.lock().await;
        let completion = match &mut *pipe {
            Pipe::BulkIn(endpoint) => receive(endpoint, length).await,
            Pipe::InterruptIn(endpoint) => receive(endpoint, length).await,
            Pipe::BulkOut(_) | Pipe::InterruptOut(_) => return Err(wrong_direction()),
        };
        match completion {
            Ok(mut read) => {
                let babble = read.len() > length;
                read.truncate(length);
                Ok(json!({"status": if babble { "babble" } else { "ok" }, "data": encode(&read)}))
            }
            Err(error) => in_failure(error),
        }
    }

    async fn transfer_out(&self, endpoint: u8, data: Vec<u8>) -> Done {
        let pipe = self.pipe(endpoint & 0x0F)?;
        let mut pipe = pipe.lock().await;
        let length = data.len();
        let sent = match &mut *pipe {
            Pipe::BulkOut(endpoint) => submit(endpoint, data).await,
            Pipe::InterruptOut(endpoint) => submit(endpoint, data).await,
            Pipe::BulkIn(_) | Pipe::InterruptIn(_) => return Err(wrong_direction()),
        };
        out_result(sent.map(|()| length))
    }

    async fn clear_halt(&self, args: &Value) -> Done {
        let number: u8 = number(args, "endpointNumber")?;
        let address = if args["direction"] == "in" {
            number | 0x80
        } else {
            number
        };
        let pipe = self.pipe(address)?;
        let mut pipe = pipe.lock().await;
        let cleared = match &mut *pipe {
            Pipe::BulkIn(endpoint) => endpoint.clear_halt().await,
            Pipe::BulkOut(endpoint) => endpoint.clear_halt().await,
            Pipe::InterruptIn(endpoint) => endpoint.clear_halt().await,
            Pipe::InterruptOut(endpoint) => endpoint.clear_halt().await,
        };
        cleared
            .map(|()| Value::Null)
            .map_err(|error| network(&error))
    }
}

fn open_pipe(interface: &Interface, address: u8, kind: TransferType) -> Result<Pipe, DomError> {
    let incoming = address & 0x80 != 0;
    let pipe = match (kind, incoming) {
        (TransferType::Bulk, true) => interface.endpoint::<Bulk, In>(address).map(Pipe::BulkIn),
        (TransferType::Bulk, false) => interface.endpoint::<Bulk, Out>(address).map(Pipe::BulkOut),
        (TransferType::Interrupt, true) => interface
            .endpoint::<Interrupt, In>(address)
            .map(Pipe::InterruptIn),
        (TransferType::Interrupt, false) => interface
            .endpoint::<Interrupt, Out>(address)
            .map(Pipe::InterruptOut),
        (TransferType::Isochronous | TransferType::Control, _) => {
            return Err(DomError::unsupported("This endpoint's transfer type"));
        }
    };
    pipe.map_err(|_| DomError::new("InvalidStateError", "The endpoint is in use."))
}

/// Reads up to `length`, asking for whole packets as the system needs.
async fn receive<T: nusb::transfer::BulkOrInterrupt>(
    endpoint: &mut Endpoint<T, In>,
    length: usize,
) -> Result<Vec<u8>, TransferError> {
    let packet = endpoint.max_packet_size().max(1);
    let asked = length.max(1).div_ceil(packet) * packet;
    endpoint.submit(Buffer::new(asked));
    endpoint
        .next_complete()
        .await
        .into_result()
        .map(Buffer::into_vec)
}

async fn submit<T: nusb::transfer::BulkOrInterrupt>(
    endpoint: &mut Endpoint<T, Out>,
    data: Vec<u8>,
) -> Result<(), TransferError> {
    endpoint.submit(Buffer::from(data));
    endpoint.next_complete().await.into_result().map(|_| ())
}

#[cfg(not(windows))]
async fn device_control_in(device: &Device, data: ControlIn) -> Result<Vec<u8>, TransferError> {
    device.control_in(data, CONTROL_WAIT).await
}

#[cfg(windows)]
async fn device_control_in(_: &Device, _: ControlIn) -> Result<Vec<u8>, TransferError> {
    Err(TransferError::InvalidArgument)
}

#[cfg(not(windows))]
async fn device_control_out(device: &Device, data: ControlOut<'_>) -> Result<(), TransferError> {
    device.control_out(data, CONTROL_WAIT).await
}

#[cfg(windows)]
async fn device_control_out(_: &Device, _: ControlOut<'_>) -> Result<(), TransferError> {
    Err(TransferError::InvalidArgument)
}
