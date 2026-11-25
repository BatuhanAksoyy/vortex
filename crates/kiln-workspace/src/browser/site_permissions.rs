//! What the user let each site use, kept with the browser's data: the
//! browser forgets permissions Kiln sets once it is closed.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use kiln_browser::Permission;

const FILE: &str = "kiln-permissions.json";

/// By origin, then by permission: whether it was allowed.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SitePermissions {
    path: Option<PathBuf>,
    sites: BTreeMap<String, BTreeMap<Permission, bool>>,
}

impl SitePermissions {
    /// What was kept in `profile`; nothing, when it was never written or
    /// cannot be read.
    pub fn load(profile: &Path) -> Self {
        let path = profile.join(FILE);
        let read = std::fs::read_to_string(&path).ok();
        let sites = read
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            sites,
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let text = serde_json::to_string_pretty(&self.sites)?;
        std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
    }

    /// The answer for all of `permissions`, when the user gave the same one
    /// for each.
    pub fn answer(&self, origin: &str, permissions: &[Permission]) -> Option<bool> {
        let site = self.sites.get(origin)?;
        let mut answers = permissions.iter().map(|permission| site.get(permission));
        let first = *answers.next()??;
        answers
            .all(|answer| answer == Some(&first))
            .then_some(first)
    }

    pub fn decide(&mut self, origin: &str, permissions: &[Permission], allowed: bool) {
        let site = self.sites.entry(origin.to_owned()).or_default();
        for permission in permissions {
            site.insert(*permission, allowed);
        }
    }

    /// Forgets the site, and says what it had.
    pub fn forget(&mut self, origin: &str) -> Vec<Permission> {
        let site = self.sites.remove(origin).unwrap_or_default();
        site.into_keys().collect()
    }

    pub fn of(&self, origin: &str) -> impl Iterator<Item = (Permission, bool)> + '_ {
        let site = self.sites.get(origin).into_iter().flatten();
        site.map(|(permission, allowed)| (*permission, *allowed))
    }

    pub fn all(&self) -> impl Iterator<Item = (&str, Permission, bool)> + '_ {
        self.sites.iter().flat_map(|(origin, site)| {
            site.iter()
                .map(move |(permission, allowed)| (origin.as_str(), *permission, *allowed))
        })
    }
}

/// The site, as people know it.
pub(super) fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#'])
        .next()
        .filter(|host| !host.is_empty())
        .unwrap_or("This page")
}

/// The page's origin, for pages that have one a permission can be kept for.
pub(super) fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|host| !host.is_empty())?;
    let host = host.rsplit('@').next().unwrap_or(host);
    Some(format!("{scheme}://{}", host.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_kept_by_site() {
        let dir = tempfile::tempdir().unwrap();
        let mut kept = SitePermissions::load(dir.path());
        let both = [Permission::Camera, Permission::Microphone];
        kept.decide("https://meet.example", &both, true);
        kept.decide("https://meet.example", &[Permission::Geolocation], false);
        kept.save().unwrap();
        let kept = SitePermissions::load(dir.path());
        assert_eq!(kept.answer("https://meet.example", &both), Some(true));
        let mixed = [Permission::Camera, Permission::Geolocation];
        assert_eq!(kept.answer("https://meet.example", &mixed), None);
        assert_eq!(kept.answer("https://other.example", &both), None);
        assert_eq!(kept.all().count(), 3);
    }

    #[test]
    fn only_web_pages_have_an_origin() {
        assert_eq!(
            origin("http://User@LocalHost:5173/a?b").as_deref(),
            Some("http://localhost:5173")
        );
        assert_eq!(origin("file:///tmp/a.html"), None);
        assert_eq!(host("http://localhost:5173/pair?x"), "localhost:5173");
        assert_eq!(host("file:///tmp/a.html"), "This page");
        assert_eq!(origin("about:blank"), None);
    }
}
