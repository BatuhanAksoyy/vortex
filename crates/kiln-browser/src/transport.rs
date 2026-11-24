//! Messages to and from the browser: JSON, each ended by a zero byte, over
//! the process's own pipes.

use std::{
    collections::HashMap,
    io::{BufRead as _, BufReader, Read, Write},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

use anyhow::{Context as _, anyhow};
use async_channel::Sender;
use serde_json::{Value, json};

use crate::event::{self, PageEvent};

type Reply = anyhow::Result<Value>;

struct Listener {
    /// The page's own frame, whose events count as the page's.
    target: String,
    events: Sender<PageEvent>,
}

#[derive(Default)]
struct Routes {
    waiting: HashMap<u64, Sender<Reply>>,
    /// By the session attached to the page.
    pages: HashMap<String, Listener>,
    ended: bool,
}

pub(crate) struct Transport {
    outgoing: Mutex<mpsc::Sender<Vec<u8>>>,
    routes: Mutex<Routes>,
    next: AtomicU64,
}

impl Transport {
    /// Speaks over `input` and `output` until the browser closes them.
    pub fn start(
        input: impl Write + Send + 'static,
        output: impl Read + Send + 'static,
    ) -> Arc<Self> {
        let (outgoing, queue) = mpsc::channel();
        let transport = Arc::new(Self {
            outgoing: Mutex::new(outgoing),
            routes: Mutex::default(),
            next: AtomicU64::new(1),
        });
        std::thread::spawn(move || write_all(&queue, input));
        let reading = Arc::downgrade(&transport);
        std::thread::spawn(move || read_all(&reading, output));
        transport
    }

    pub fn ended(&self) -> bool {
        lock(&self.routes).ended
    }

    /// Sends a command without waiting for its result.
    pub fn send(&self, method: &str, params: &Value, session: Option<&str>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.post(id, method, params, session);
    }

    /// Sends a command and waits for its result.
    pub async fn call(&self, method: &str, params: Value, session: Option<&str>) -> Reply {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (reply, answered) = async_channel::bounded(1);
        {
            let mut routes = lock(&self.routes);
            if routes.ended {
                return Err(anyhow!("The browser has ended."));
            }
            routes.waiting.insert(id, reply);
        }
        self.post(id, method, &params, session);
        answered.recv().await.context("The browser has ended.")?
    }

    pub fn listen(&self, session: &str, target: &str, events: Sender<PageEvent>) {
        let listener = Listener {
            target: target.to_owned(),
            events,
        };
        lock(&self.routes)
            .pages
            .insert(session.to_owned(), listener);
    }

    pub fn forget(&self, session: &str) {
        lock(&self.routes).pages.remove(session);
    }

    fn post(&self, id: u64, method: &str, params: &Value, session: Option<&str>) {
        let mut message = json!({"id": id, "method": method, "params": params});
        if let Some(session) = session {
            message["sessionId"] = session.into();
        }
        let mut bytes = message.to_string().into_bytes();
        bytes.push(0);
        // An error means the writer ended with the browser.
        let _ = lock(&self.outgoing).send(bytes);
    }

    fn receive(&self, message: &[u8]) {
        let Ok(message) = serde_json::from_slice::<Value>(message) else {
            log::warn!("the browser sent a message that is not JSON");
            return;
        };
        match message["id"].as_u64() {
            Some(id) => self.answer(id, &message),
            None => self.report(&message),
        }
    }

    fn answer(&self, id: u64, message: &Value) {
        let waiting = lock(&self.routes).waiting.remove(&id);
        let error = message["error"]["message"].as_str();
        match (waiting, error) {
            (Some(reply), None) => drop(reply.try_send(Ok(message["result"].clone()))),
            (Some(reply), Some(error)) => drop(reply.try_send(Err(anyhow!("{error}")))),
            (None, Some(error)) => log::debug!("the browser refused a command: {error}"),
            (None, None) => {}
        }
    }

    fn report(&self, message: &Value) {
        let method = message["method"].as_str().unwrap_or_default();
        let params = &message["params"];
        if method == "Target.targetCreated" {
            self.close_popup(&params["targetInfo"]);
        }
        let session = message["sessionId"].as_str().unwrap_or_default();
        if let Some(listener) = lock(&self.routes).pages.get(session)
            && let Some(event) = event::parse(method, params, &listener.target)
        {
            let _ = listener.events.try_send(event);
        }
    }

    /// Nothing shows a page another one opened; the opener reports its URL,
    /// which the view decides about.
    fn close_popup(&self, target: &Value) {
        if target["openerId"].is_string() {
            let close = json!({"targetId": target["targetId"]});
            self.send("Target.closeTarget", &close, None);
        }
    }

    /// Dropping the channels tells whoever waits on them.
    fn end(&self) {
        let mut routes = lock(&self.routes);
        routes.ended = true;
        routes.waiting.clear();
        routes.pages.clear();
    }
}

fn read_all(transport: &Weak<Transport>, output: impl Read) {
    for message in BufReader::new(output).split(0) {
        let (Ok(message), Some(transport)) = (message, transport.upgrade()) else {
            break;
        };
        transport.receive(&message);
    }
    if let Some(transport) = transport.upgrade() {
        transport.end();
    }
}

fn write_all(queue: &mpsc::Receiver<Vec<u8>>, mut input: impl Write) {
    for message in queue {
        if input
            .write_all(&message)
            .and_then(|()| input.flush())
            .is_err()
        {
            return;
        }
    }
}

/// The maps stay valid whatever a panicking holder did.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn replies_find_their_command_and_an_end_fails_the_rest() {
        let (theirs, ours) = std::io::pipe().unwrap();
        let (answers, mut answering) = std::io::pipe().unwrap();
        let transport = Transport::start(ours, answers);
        let browser = std::thread::spawn(move || {
            let mut messages = BufReader::new(theirs).split(0);
            let mut next =
                || -> Value { serde_json::from_slice(&messages.next().unwrap().unwrap()).unwrap() };
            let first = next();
            assert_eq!(first["sessionId"], "s");
            let reply = json!({"id": first["id"], "result": {"ok": true}});
            answering
                .write_all(format!("{reply}\0").as_bytes())
                .unwrap();
            let second = next();
            let reply = json!({"id": second["id"], "error": {"message": "no"}});
            answering
                .write_all(format!("{reply}\0").as_bytes())
                .unwrap();
            next();
        });
        let call = || block_on(transport.call("A.b", json!({}), Some("s")));
        assert_eq!(call().unwrap()["ok"], true);
        assert_eq!(call().unwrap_err().to_string(), "no");
        assert!(call().is_err(), "ended before answering");
        browser.join().unwrap();
        assert!(call().is_err());
        assert!(transport.ended());
    }
}
