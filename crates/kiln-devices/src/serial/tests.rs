//! A port used as the page would, over a pseudo-terminal whose other end
//! stands for the device. macOS sets no speed on a pseudo-terminal, so the
//! port is taken as opened.

use std::{
    io::{Read as _, Write as _},
    time::Duration,
};

use serde_json::json;
use serialport::{SerialPort as _, TTYPort};

use super::Serial;

#[test]
fn a_port_is_read_and_written_as_the_page_asks() {
    let (mut device, port) = TTYPort::pair().unwrap();
    let id = "serial:/dev/kiln-test";
    let (events, received) = async_channel::unbounded();
    let serial = Serial::new(events);
    serial.start(id, Box::new(port), 64).unwrap();
    let call = |method, args| serial.call(id, method, &args);
    let again = call("open", json!({"baudRate": 115_200})).unwrap_err();
    assert_eq!(again.name, "InvalidStateError");

    // A write waits until the other end has read it.
    let mut listening = device.try_clone_native().unwrap();
    let heard = std::thread::spawn(move || {
        let mut heard = [0; 5];
        listening.set_timeout(Duration::from_secs(5)).unwrap();
        listening.read_exact(&mut heard).unwrap();
        heard
    });
    call("write", json!({"data": "aGVsbG8="})).unwrap();
    assert_eq!(&heard.join().unwrap(), b"hello");

    device.write_all(b"ok").unwrap();
    let event = received.recv_blocking().unwrap();
    assert_eq!(event.device, id);
    assert_eq!(event.event, json!({"type": "data", "data": "b2s="}));

    call("close", json!({})).unwrap();
    let closed = call("write", json!({"data": "aGk="})).unwrap_err();
    assert_eq!(closed.name, "InvalidStateError");
}

#[test]
fn wrong_settings_are_refused() {
    let (events, _) = async_channel::unbounded();
    let serial = Serial::new(events);
    let call = |args| serial.call("serial:/dev/null", "open", &args);
    assert_eq!(call(json!({})).unwrap_err().name, "TypeError");
    let mark = call(json!({"baudRate": 9600, "parity": "mark"})).unwrap_err();
    assert_eq!(mark.name, "TypeError");
}
