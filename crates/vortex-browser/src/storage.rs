//! What the page keeps in the browser: cookies, local and session storage,
//! and `IndexedDB`, read and cleared for the site shown.

use anyhow::Context as _;
use serde_json::{Value, json};

use crate::Page;

/// Entries read from an object store at once.
const PAGE_SIZE: usize = 50;
/// A long value is cut in the list.
const MAX_VALUE: usize = 1000;

const AS_TEXT: &str = "function () {
  try { return JSON.stringify(this) ?? String(this); } catch { return String(this); }
}";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    /// Seconds since 1970; `None` lasts the session.
    pub expires: Option<f64>,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Database {
    pub name: String,
    pub stores: Vec<String>,
}

/// An object store's entries, as text, and whether there are more.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreEntries {
    pub entries: Vec<(String, String)>,
    pub more: bool,
}

impl Page {
    /// The cookies the page at `url` is sent.
    ///
    /// # Errors
    /// When the browser does not answer.
    pub async fn cookies(&self, url: &str) -> anyhow::Result<Vec<Cookie>> {
        let reply = self
            .call("Network.getCookies", json!({"urls": [url]}))
            .await?;
        let cookies = reply["cookies"].as_array().map_or_default(Vec::as_slice);
        Ok(cookies.iter().map(cookie).collect())
    }

    /// # Errors
    /// When the browser does not answer.
    pub async fn delete_cookie(&self, cookie: &Cookie) -> anyhow::Result<()> {
        let delete = json!({"name": cookie.name, "domain": cookie.domain, "path": cookie.path});
        self.call("Network.deleteCookies", delete).await?;
        Ok(())
    }

    /// Local storage, or session storage, of the document shown.
    ///
    /// # Errors
    /// When the browser does not answer.
    pub async fn web_storage(&self, local: bool) -> anyhow::Result<Vec<(String, String)>> {
        let id = self.storage_id(local).await?;
        let reply = self
            .call("DOMStorage.getDOMStorageItems", json!({"storageId": id}))
            .await?;
        let entries = reply["entries"].as_array().map_or_default(Vec::as_slice);
        Ok(entries
            .iter()
            .filter_map(|entry| Some((text(&entry[0])?, cut(&text(&entry[1])?))))
            .collect())
    }

    /// # Errors
    /// When the browser does not answer.
    pub async fn remove_web_storage(&self, local: bool, key: &str) -> anyhow::Result<()> {
        let id = self.storage_id(local).await?;
        let remove = json!({"storageId": id, "key": key});
        self.call("DOMStorage.removeDOMStorageItem", remove).await?;
        Ok(())
    }

    /// The document's `IndexedDB` databases and their object stores.
    ///
    /// # Errors
    /// When the browser does not answer.
    pub async fn databases(&self) -> anyhow::Result<Vec<Database>> {
        let key = self.storage_key().await?;
        let names = self
            .call("IndexedDB.requestDatabaseNames", json!({"storageKey": key}))
            .await?;
        let mut databases = Vec::new();
        for name in names["databaseNames"]
            .as_array()
            .map_or_default(Vec::as_slice)
        {
            let Some(name) = text(name) else { continue };
            let ask = json!({"storageKey": key, "databaseName": name});
            let database = self.call("IndexedDB.requestDatabase", ask).await?;
            let stores = database["databaseWithObjectStores"]["objectStores"]
                .as_array()
                .map_or_default(Vec::as_slice);
            let stores = stores
                .iter()
                .filter_map(|store| text(&store["name"]))
                .collect();
            databases.push(Database { name, stores });
        }
        Ok(databases)
    }

    /// The first entries of an object store, keys and values as text.
    ///
    /// # Errors
    /// When the browser does not answer.
    pub async fn store_entries(&self, database: &str, store: &str) -> anyhow::Result<StoreEntries> {
        let ask = json!({
            "storageKey": self.storage_key().await?,
            "databaseName": database,
            "objectStoreName": store,
            "skipCount": 0,
            "pageSize": PAGE_SIZE,
        });
        let reply = self.call("IndexedDB.requestData", ask).await?;
        let mut entries = Vec::new();
        for entry in reply["objectStoreDataEntries"]
            .as_array()
            .map_or_default(Vec::as_slice)
        {
            let key = self.remote_text(&entry["key"]).await;
            let value = self.remote_text(&entry["value"]).await;
            entries.push((key, cut(&value)));
        }
        Ok(StoreEntries {
            entries,
            more: reply["hasMore"] == true,
        })
    }

    /// Clears everything the site keeps: cookies, storage, databases and
    /// caches.
    ///
    /// # Errors
    /// When the browser does not answer.
    pub async fn clear_site_data(&self, origin: &str) -> anyhow::Result<()> {
        let clear = json!({"origin": origin, "storageTypes": "all"});
        self.call("Storage.clearDataForOrigin", clear).await?;
        Ok(())
    }

    async fn storage_key(&self) -> anyhow::Result<String> {
        // Without a frame the browser looks for a worker.
        let tree = self.call("Page.getFrameTree", json!({})).await?;
        let frame = text(&tree["frameTree"]["frame"]["id"]).context("the page has no frame")?;
        let ask = json!({"frameId": frame});
        let reply = self.call("Storage.getStorageKeyForFrame", ask).await?;
        text(&reply["storageKey"]).context("the page has no storage")
    }

    async fn storage_id(&self, local: bool) -> anyhow::Result<Value> {
        Ok(json!({"storageKey": self.storage_key().await?, "isLocalStorage": local}))
    }

    /// A value of the page's, as text.
    async fn remote_text(&self, remote: &Value) -> String {
        if let Some(id) = remote["objectId"].as_str() {
            let ask =
                json!({"objectId": id, "functionDeclaration": AS_TEXT, "returnByValue": true});
            if let Ok(reply) = self.call("Runtime.callFunctionOn", ask).await {
                let _ = self
                    .call("Runtime.releaseObject", json!({"objectId": id}))
                    .await;
                return text(&reply["result"]["value"]).unwrap_or_default();
            }
        }
        match &remote["value"] {
            Value::String(value) => value.clone(),
            Value::Null => text(&remote["description"]).unwrap_or_default(),
            value => value.to_string(),
        }
    }
}

fn cookie(cookie: &Value) -> Cookie {
    let expires = cookie["expires"]
        .as_f64()
        .filter(|_| cookie["session"] != true);
    Cookie {
        name: text(&cookie["name"]).unwrap_or_default(),
        value: cut(&text(&cookie["value"]).unwrap_or_default()),
        domain: text(&cookie["domain"]).unwrap_or_default(),
        path: text(&cookie["path"]).unwrap_or_default(),
        expires,
        http_only: cookie["httpOnly"] == true,
        secure: cookie["secure"] == true,
        same_site: text(&cookie["sameSite"]),
    }
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_VALUE) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod installed;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_cookie_has_no_end() {
        let read = cookie(&json!({
            "name": "id", "value": "7", "domain": "a.test", "path": "/",
            "expires": -1, "session": true, "httpOnly": true, "secure": false,
        }));
        assert_eq!((read.expires, read.http_only), (None, true));
    }
}
