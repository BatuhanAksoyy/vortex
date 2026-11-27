//! Vortex's own updates: the newest release on GitHub, whether it is newer
//! than the running Vortex, and putting it in place of the running app. No UI
//! dependencies.

mod install;

use std::{cmp::Ordering, fmt, process::Command, process::Stdio};

use anyhow::{Context as _, bail};
use serde::Deserialize;

pub use install::{install, relaunch, running_bundle};

/// The running Vortex's version.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// Where Vortex is published, as `https://github.com/<owner>/<repo>`.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const CHECK_SECONDS: &str = "15";

/// `major.minor.patch`; a `v` before it and anything after a `-` or `+`
/// are left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version([u64; 3]);

impl Version {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('v');
        let core = text.split(['-', '+']).next()?;
        let mut parts = core.split('.').map(|part| part.parse().ok());
        let version = [parts.next()??, parts.next()??, parts.next()??];
        parts.next().is_none().then_some(Self(version))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.0;
        write!(f, "{major}.{minor}.{patch}")
    }
}

/// A published release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// What changed, as Markdown.
    pub notes: String,
    /// The release's page on GitHub.
    pub page: String,
    /// The archive of the app for this machine, if the release has one.
    pub download: Option<String>,
}

impl Release {
    /// Whether it is newer than the running Vortex.
    #[must_use]
    pub fn is_newer(&self) -> bool {
        Version::parse(CURRENT)
            .is_none_or(|current| self.version.cmp(&current) == Ordering::Greater)
    }
}

#[derive(Deserialize)]
struct Published {
    tag_name: String,
    #[serde(default)]
    body: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// The newest release, asked of GitHub.
pub fn latest() -> anyhow::Result<Release> {
    let path = REPOSITORY.trim_start_matches("https://github.com/");
    let url = format!("https://api.github.com/repos/{path}/releases/latest");
    parse_release(&fetch(&url)?, &asset_name)
}

/// The release GitHub describes in `json`, with the archive `name_for`
/// names for its version.
fn parse_release(json: &str, name_for: &dyn Fn(Version) -> String) -> anyhow::Result<Release> {
    let published: Published = serde_json::from_str(json).context("an unexpected release")?;
    if published.draft || published.prerelease {
        bail!("the newest release is not final");
    }
    let Some(version) = Version::parse(&published.tag_name) else {
        bail!("the release `{}` is not a version", published.tag_name);
    };
    let name = name_for(version);
    let download = published
        .assets
        .into_iter()
        .find(|asset| asset.name == name);
    Ok(Release {
        version,
        notes: published.body,
        page: published.html_url,
        download: download.map(|asset| asset.browser_download_url),
    })
}

/// The archive a release keeps the app for this machine in; the release
/// workflow names them so.
fn asset_name(version: Version) -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    format!("Vortex-{version}-macos-{arch}.zip")
}

fn fetch(url: &str) -> anyhow::Result<String> {
    let output = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location"])
        .args(["--max-time", CHECK_SECONDS])
        .args(["--header", "Accept: application/vnd.github+json", url])
        .stdin(Stdio::null())
        .output()
        .context("cannot run curl")?;
    if !output.status.success() {
        bail!(
            "cannot read {url}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::{Version, parse_release};

    #[test]
    fn versions_compare_by_their_numbers() {
        let parse = |text| Version::parse(text).unwrap();
        assert!(parse("v0.10.0") > parse("0.9.3"));
        assert_eq!(parse("1.2.3-beta.1"), parse("1.2.3"));
        assert_eq!(parse("v1.2.3").to_string(), "1.2.3");
        assert_eq!(Version::parse("1.2"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("latest"), None);
    }

    const PUBLISHED: &str = r#"{
        "tag_name": "v0.2.0", "body": "- Settings page", "draft": false, "prerelease": false,
        "html_url": "https://github.com/o/vortex/releases/tag/v0.2.0",
        "assets": [
            {"name": "Vortex-0.2.0-macos-x86_64.zip", "browser_download_url": "https://x/intel.zip"},
            {"name": "Vortex-0.2.0-macos-arm64.zip", "browser_download_url": "https://x/arm.zip"}
        ]
    }"#;

    #[test]
    fn a_release_offers_the_archive_for_this_machine() {
        let name = |version| format!("Vortex-{version}-macos-arm64.zip");
        let release = parse_release(PUBLISHED, &name).unwrap();
        assert_eq!(release.version, Version::parse("0.2.0").unwrap());
        assert_eq!(release.download.as_deref(), Some("https://x/arm.zip"));
        assert_eq!(release.notes, "- Settings page");
        let other = |_| "Vortex-linux.zip".to_owned();
        assert_eq!(parse_release(PUBLISHED, &other).unwrap().download, None);
        let draft = PUBLISHED.replace(r#""draft": false"#, r#""draft": true"#);
        assert!(parse_release(&draft, &name).is_err());
    }
}
