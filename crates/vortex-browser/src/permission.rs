//! What a page may use only with the user's leave: the camera, the
//! microphone, the location, notifications and MIDI. A browser without a
//! window asks no one and says no, so the page's script asks Vortex first
//! and Vortex sets the permission before the call goes on.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Page, event::PageEvent};

pub(crate) const SOURCE: &str = include_str!("permission.js");
pub(crate) const BINDING: &str = "vortexPermission";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    Camera,
    Microphone,
    Geolocation,
    Notifications,
    Midi,
    /// MIDI devices, with messages that can change their settings.
    MidiSysex,
}

impl Permission {
    pub const ALL: [Self; 6] = [
        Self::Camera,
        Self::Microphone,
        Self::Geolocation,
        Self::Notifications,
        Self::Midi,
        Self::MidiSysex,
    ];

    /// What the page would use, as the user is asked about it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Camera => "your camera",
            Self::Microphone => "your microphone",
            Self::Geolocation => "your location",
            Self::Notifications => "notifications",
            Self::Midi => "your MIDI devices",
            Self::MidiSysex => "your MIDI devices, with full control",
        }
    }

    /// Its name, as a list of them shows it.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Camera => "Camera",
            Self::Microphone => "Microphone",
            Self::Geolocation => "Location",
            Self::Notifications => "Notifications",
            Self::Midi => "MIDI devices",
            Self::MidiSysex => "MIDI devices, full control",
        }
    }

    /// The browser's name for it.
    fn descriptor(self) -> Value {
        match self {
            Self::Camera => json!({"name": "camera"}),
            Self::Microphone => json!({"name": "microphone"}),
            Self::Geolocation => json!({"name": "geolocation"}),
            Self::Notifications => json!({"name": "notifications"}),
            Self::Midi => json!({"name": "midi"}),
            Self::MidiSysex => json!({"name": "midi", "sysex": true}),
        }
    }
}

/// A page waiting on the user's leave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequest {
    pub permissions: Vec<Permission>,
    id: u64,
    /// The document's script, which goes on once answered.
    context: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Message {
    Ask {
        id: u64,
        permissions: Vec<Permission>,
    },
    Notify {
        title: String,
        body: String,
    },
}

/// The event in a call of the binding, if it is this one's.
pub(crate) fn parse(params: &Value) -> Option<PageEvent> {
    if params["name"] != BINDING {
        return None;
    }
    let message = serde_json::from_str(params["payload"].as_str()?).ok()?;
    Some(match message {
        Message::Ask { id, permissions } => PageEvent::Permission(PermissionRequest {
            permissions,
            id,
            context: params["executionContextId"].as_i64()?,
        }),
        Message::Notify { title, body } => PageEvent::Notification { title, body },
    })
}

impl Page {
    /// Sets the permissions `origin` asked for, then lets the page go on,
    /// even when the browser did not take them.
    ///
    /// # Errors
    /// When the browser does not take a permission.
    pub async fn permit(
        &self,
        request: &PermissionRequest,
        origin: &str,
        granted: bool,
    ) -> anyhow::Result<()> {
        let mut set = Ok(());
        for permission in &request.permissions {
            let done = self.set_permission(Some(origin), *permission, Some(granted));
            set = set.and(done.await);
        }
        let answer = json!({
            "expression": format!("vortexPermitted({})", request.id),
            "contextId": request.context,
        });
        self.send("Runtime.evaluate", &answer);
        set
    }

    /// What `origin`, or every site, may use; `None` leaves it to the user.
    ///
    /// # Errors
    /// When the browser does not take the permission.
    pub async fn set_permission(
        &self,
        origin: Option<&str>,
        permission: Permission,
        granted: Option<bool>,
    ) -> anyhow::Result<()> {
        let setting = match granted {
            Some(true) => "granted",
            Some(false) => "denied",
            None => "prompt",
        };
        let mut set = json!({"permission": permission.descriptor(), "setting": setting});
        if let Some(origin) = origin {
            set["origin"] = json!(origin);
        }
        // Permissions are the browser's, not the page's.
        self.transport
            .call("Browser.setPermission", set, None)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod installed;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_asks_in_its_document() {
        let payload = r#"{"ask":{"id":3,"permissions":["camera","midiSysex"]}}"#;
        let call = json!({"name": BINDING, "payload": payload, "executionContextId": 8});
        let Some(PageEvent::Permission(request)) = parse(&call) else {
            panic!("no request");
        };
        assert_eq!(
            request.permissions,
            [Permission::Camera, Permission::MidiSysex]
        );
        assert_eq!((request.id, request.context), (3, 8));
        let unknown =
            json!({"name": BINDING, "payload": r#"{"ask":{"id":1,"permissions":["x"]}}"#});
        assert_eq!(parse(&unknown), None);
    }
}
