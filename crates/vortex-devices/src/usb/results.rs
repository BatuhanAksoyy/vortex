//! What a USB call comes to in the page's terms: a setup packet read from
//! it, and a transfer's status or the `DOMException` it fails with.

use nusb::transfer::{ControlType, Recipient, TransferError};
use serde_json::{Value, json};

use crate::{DomError, Done, number};

pub(super) fn parse_setup(
    setup: &Value,
) -> Result<(ControlType, Recipient, u8, u16, u16), DomError> {
    let control_type = match setup["requestType"].as_str() {
        Some("standard") => ControlType::Standard,
        Some("class") => ControlType::Class,
        Some("vendor") => ControlType::Vendor,
        _ => return Err(DomError::argument("requestType")),
    };
    let recipient = match setup["recipient"].as_str() {
        Some("device") => Recipient::Device,
        Some("interface") => Recipient::Interface,
        Some("endpoint") => Recipient::Endpoint,
        Some("other") => Recipient::Other,
        _ => return Err(DomError::argument("recipient")),
    };
    Ok((
        control_type,
        recipient,
        number(setup, "request")?,
        number(setup, "value")?,
        number(setup, "index")?,
    ))
}

/// A stall is a result, not a failure; the page reads it from `status`.
pub(super) fn in_failure(error: TransferError) -> Done {
    match error {
        TransferError::Stall => Ok(json!({"status": "stall", "data": ""})),
        error => Err(transfer_error(error)),
    }
}

pub(super) fn out_result(sent: Result<usize, TransferError>) -> Done {
    match sent {
        Ok(written) => Ok(json!({"status": "ok", "bytesWritten": written})),
        Err(TransferError::Stall) => Ok(json!({"status": "stall", "bytesWritten": 0})),
        Err(error) => Err(transfer_error(error)),
    }
}

pub(super) fn transfer_error(error: TransferError) -> DomError {
    match error {
        TransferError::Disconnected => DomError::not_found(),
        TransferError::Cancelled => DomError::new("AbortError", "The transfer was cancelled."),
        _ => DomError::new("NetworkError", "A transfer error has occurred."),
    }
}

pub(super) fn network(error: &nusb::Error) -> DomError {
    DomError::new("NetworkError", format!("The operation failed: {error}"))
}

pub(super) fn unclaimed() -> DomError {
    DomError::new("InvalidStateError", "The interface must be claimed first.")
}

pub(super) fn wrong_direction() -> DomError {
    DomError::new(
        "InvalidAccessError",
        "The endpoint's direction does not match.",
    )
}
