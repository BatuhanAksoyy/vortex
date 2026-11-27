//! What a page asks of its user beyond drawing: answers, files and choices.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::json;

use crate::{Page, download, guest::Select};

impl Page {
    /// Answers the dialog the page waits on; `text` is a prompt's.
    pub fn answer_dialog(&self, accept: bool, text: &str) {
        let answer = json!({"accept": accept, "promptText": text});
        self.send("Page.handleJavaScriptDialog", &answer);
    }

    pub fn choose(&self, select: &Select, index: i64) {
        let choose = json!({
            "expression": format!("vortexChoose({index})"),
            "contextId": select.context,
        });
        self.send("Runtime.evaluate", &choose);
    }

    /// Gives the file field at `node` these files.
    pub fn give_files(&self, node: i64, files: &[PathBuf]) {
        let files: Vec<_> = files.iter().map(|file| file.to_string_lossy()).collect();
        let give = json!({"files": files, "backendNodeId": node});
        self.send("DOM.setFileInputFiles", &give);
    }

    /// Pastes a picture where the caret is, as far as the page takes one.
    pub fn paste_image(&self, media_type: &str, data: &[u8]) {
        let data = base64::engine::general_purpose::STANDARD.encode(data);
        let expression = format!(
            "(() => {{ const bytes = Uint8Array.from(atob('{data}'), c => c.charCodeAt(0)); \
             const held = new DataTransfer(); \
             held.items.add(new File([bytes], 'image.png', {{ type: '{media_type}' }})); \
             (document.activeElement ?? document.body).dispatchEvent(new ClipboardEvent('paste', \
             {{ clipboardData: held, bubbles: true, cancelable: true, composed: true }})); }})()"
        );
        self.send("Runtime.evaluate", &json!({"expression": expression}));
    }

    pub fn cancel_download(&self, id: &str) {
        let cancel = json!({"guid": id});
        self.transport.send("Browser.cancelDownload", &cancel, None);
    }

    /// Puts a received download in `folder`.
    pub fn keep_download(&self, id: &str, folder: &Path, name: &str) -> anyhow::Result<PathBuf> {
        download::keep(&self.received.join(id), folder, name)
    }
}
