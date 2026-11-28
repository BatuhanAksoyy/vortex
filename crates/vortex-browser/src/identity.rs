//! How the browser names itself to sites. Without a window it says
//! `HeadlessChrome` and sends no brands in its client hints, and sign-in
//! pages such as Google's turn such a browser away as unsafe; so a page
//! names itself as the same browser with a window would.

use anyhow::Context as _;
use serde_json::{Value, json};

/// The browser's name and version, from `Browser.getVersion`.
#[derive(Clone, Debug)]
pub(crate) struct Identity {
    agent: String,
    /// Its full version, as `157.0.7470.12`.
    version: String,
}

impl Identity {
    pub(crate) fn from_version(reply: &Value) -> anyhow::Result<Self> {
        let agent = reply["userAgent"]
            .as_str()
            .context("the browser sent no userAgent")?
            .replace("HeadlessChrome", "Chrome");
        let product = reply["product"].as_str().unwrap_or_default();
        let version = product.split_once('/').map_or("", |(_, version)| version);
        Ok(Self {
            agent,
            version: version.to_owned(),
        })
    }

    /// What `Emulation.setUserAgentOverride` takes, as a phone or as the
    /// computer it runs on.
    pub(crate) fn emulation(&self, phone: bool) -> Value {
        let agent = if phone {
            phone_agent(&self.agent)
        } else {
            self.agent.clone()
        };
        let major = self.version.split('.').next().unwrap_or_default();
        let brands = |version: &str| {
            json!([
                {"brand": "Not_A Brand", "version": if version == major { "99" } else { "99.0.0.0" }},
                {"brand": "Chromium", "version": version},
                {"brand": "Google Chrome", "version": version},
            ])
        };
        let (platform, model) = if phone {
            ("Android", "K")
        } else {
            (platform(), "")
        };
        json!({
            "userAgent": agent,
            "userAgentMetadata": {
                "brands": brands(major),
                "fullVersionList": brands(&self.version),
                "fullVersion": self.version,
                "platform": platform,
                "platformVersion": if phone { "10.0.0" } else { "" },
                "architecture": if phone { "" } else { architecture() },
                "bitness": if phone { "" } else { "64" },
                "model": model,
                "mobile": phone,
            },
        })
    }
}

/// The system as client hints name it.
fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        _ => "Linux",
    }
}

fn architecture() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" | "arm" => "arm",
        _ => "x86",
    }
}

fn phone_agent(agent: &str) -> String {
    let system = agent
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map_or("", |(system, _)| system);
    if system.is_empty() {
        return agent.to_owned();
    }
    agent
        .replacen(system, "Linux; Android 10; K", 1)
        .replace(" Safari/", " Mobile Safari/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Identity {
        let reply = json!({
            "product": "HeadlessChrome/158.0.7470.12",
            "userAgent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/158.0.0.0 Safari/537.36",
        });
        Identity::from_version(&reply).unwrap()
    }

    #[test]
    fn a_page_names_itself_as_chrome_with_its_brands() {
        let emulation = identity().emulation(false);
        let agent = emulation["userAgent"].as_str().unwrap();
        assert!(agent.contains(" Chrome/158.0.0.0 "), "{agent}");
        let hints = &emulation["userAgentMetadata"];
        assert_eq!(
            hints["brands"][2],
            json!({"brand": "Google Chrome", "version": "158"})
        );
        assert_eq!(hints["fullVersionList"][1]["version"], "158.0.7470.12");
        assert_eq!(hints["mobile"], false);
        assert_ne!(hints["platform"].as_str().unwrap(), "");
    }

    #[test]
    fn a_phone_is_named_with_the_same_version() {
        let phone = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/158.0.0.0 Mobile Safari/537.36";
        let emulation = identity().emulation(true);
        assert_eq!(emulation["userAgent"], phone);
        let hints = &emulation["userAgentMetadata"];
        assert_eq!(
            (hints["platform"].as_str(), hints["mobile"].as_bool()),
            (Some("Android"), Some(true))
        );
        assert_eq!(hints["brands"][1]["version"], "158");
    }
}
