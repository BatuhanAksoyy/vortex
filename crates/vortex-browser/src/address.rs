//! What the user types as an address.

use anyhow::bail;

const SCHEMES: [&str; 3] = ["http", "https", "file"];

/// The URL `typed` stands for: `localhost:3000` without its scheme, and any
/// other host over https.
pub fn normalize(typed: &str) -> anyhow::Result<String> {
    let typed = typed.trim();
    if typed.is_empty() || typed.contains(char::is_whitespace) {
        bail!("Not an address: {typed}");
    }
    if typed == "about:blank" {
        return Ok(typed.to_owned());
    }
    if let Some((scheme, _)) = typed.split_once("://") {
        if SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) {
            return Ok(typed.to_owned());
        }
        bail!("Cannot open {scheme} addresses.");
    }
    let host = typed.split(['/', '?', '#']).next().unwrap_or(typed);
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) => (name, Some(port)),
        None => (host, None),
    };
    if port.is_some_and(|port| port.parse::<u16>().is_err()) {
        bail!("Not an address: {typed}");
    }
    let scheme = if is_local(name) || port.is_some() {
        "http"
    } else {
        "https"
    };
    Ok(format!("{scheme}://{typed}"))
}

fn is_local(name: &str) -> bool {
    name == "localhost"
        || name.ends_with(".localhost")
        || name.parse::<std::net::Ipv4Addr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn a_scheme_is_added_where_it_is_missing() {
        for (typed, url) in [
            ("localhost:3000/a?b", "http://localhost:3000/a?b"),
            ("localhost", "http://localhost"),
            ("127.0.0.1:8080", "http://127.0.0.1:8080"),
            ("myserver:8080", "http://myserver:8080"),
            (" example.com/a:b ", "https://example.com/a:b"),
            ("https://example.com", "https://example.com"),
            ("about:blank", "about:blank"),
        ] {
            assert_eq!(normalize(typed).unwrap(), url);
        }
    }

    #[test]
    fn what_is_no_page_is_refused() {
        for typed in [
            "",
            "two words",
            "javascript:alert(1)",
            "chrome://version",
            "a:b",
        ] {
            assert!(normalize(typed).is_err(), "{typed}");
        }
    }
}
