//! `vortex mcp`: the agent's end of the bridge.

use std::io::{self, BufRead as _, BufReader, Read, Write};

use anyhow::Context as _;

use crate::{SOCKET_VAR, TOKEN_VAR};

/// Hands the agent's lines to Vortex and Vortex's back, until either ends.
pub fn relay() -> anyhow::Result<()> {
    let socket = std::env::var_os(SOCKET_VAR).context("not started by Vortex")?;
    let token = std::env::var(TOKEN_VAR).context("not started by Vortex")?;
    let stream = std::os::unix::net::UnixStream::connect(&socket)
        .with_context(|| format!("cannot reach Vortex at {}", socket.display()))?;
    let mut to_vortex = stream.try_clone()?;
    writeln!(to_vortex, "{token}")?;
    std::thread::spawn(move || {
        let _ = pass_lines(io::stdin().lock(), &mut to_vortex);
        let _ = to_vortex.shutdown(std::net::Shutdown::Write);
    });
    pass_lines(stream, &mut io::stdout().lock())
}

/// Line by line, so each message goes as soon as it is whole.
fn pass_lines(from: impl Read, to: &mut impl Write) -> anyhow::Result<()> {
    for line in BufReader::new(from).lines() {
        writeln!(to, "{}", line?)?;
        to.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_pass_whole() {
        let mut out = Vec::new();
        pass_lines(&b"{\"a\":1}\n{\"b\":2}"[..], &mut out).unwrap();
        assert_eq!(out, b"{\"a\":1}\n{\"b\":2}\n");
    }
}
