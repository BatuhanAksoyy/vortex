//! A tool server talks to Vortex through the socket.

#[cfg(test)]
mod bridge {
    use std::{
        io::{BufRead as _, BufReader, Write as _},
        os::unix::net::UnixStream,
        path::PathBuf,
    };

    use serde_json::{Value, json};
    use vortex_bridge::{Bridge, Browse, Reply, Request};

    fn connect(
        server: &vortex_agent::ToolServer,
        token: &str,
    ) -> (UnixStream, impl FnMut() -> Value) {
        let socket = &server
            .env
            .iter()
            .find(|(name, _)| name == "VORTEX_BRIDGE")
            .unwrap()
            .1;
        let mut stream = UnixStream::connect(socket).unwrap();
        writeln!(stream, "{token}").unwrap();
        let mut lines = BufReader::new(stream.try_clone().unwrap()).lines();
        let next = move || serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
        (stream, next)
    }

    #[test]
    fn calls_reach_vortex_with_the_chats_token() {
        let dir = tempfile::tempdir().unwrap();
        let (bridge, calls) = Bridge::start(dir.path(), PathBuf::from("vortex")).unwrap();
        let (server, token) = bridge.server().unwrap();
        assert_eq!(server.args, ["mcp"]);
        let (mut stream, mut next) = connect(&server, &token);

        let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
        writeln!(stream, "{list}").unwrap();
        assert!(next()["result"]["tools"].as_array().unwrap().len() > 5);

        let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "browser_navigate", "arguments": {"url": "localhost:3000"}}});
        writeln!(stream, "{call}").unwrap();
        let received = calls.recv_blocking().unwrap();
        assert_eq!(received.token, token);
        let url = "localhost:3000".to_owned();
        assert_eq!(received.request, Request::Browse(Browse::Navigate { url }));
        received.reply.send_blocking(Reply::text("opened")).unwrap();
        let answer = next();
        assert_eq!(answer["id"], 2);
        assert_eq!(answer["result"]["content"][0]["text"], "opened");
    }

    #[test]
    fn a_stranger_is_not_served() {
        let dir = tempfile::tempdir().unwrap();
        let (bridge, _calls) = Bridge::start(dir.path(), PathBuf::from("vortex")).unwrap();
        let (server, _) = bridge.server().unwrap();
        let socket = &server.env[0].1;
        let mut stream = UnixStream::connect(socket).unwrap();
        writeln!(stream, "guess").unwrap();
        writeln!(stream, r#"{{"jsonrpc":"2.0","id":1,"method":"ping"}}"#).unwrap();
        let mut answer = String::new();
        let read = BufReader::new(stream).read_line(&mut answer).unwrap();
        assert_eq!(read, 0);
    }
}
