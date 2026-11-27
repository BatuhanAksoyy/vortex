//! A page served on this machine, for tests that need an origin, which a
//! file lacks.

use std::{
    io::{Read as _, Write as _},
    net::TcpListener,
};

/// Serves `page` at every path; the origin it is served from.
pub fn serve(page: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{page}",
                page.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    origin
}
