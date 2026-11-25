//! Chrome's own storage, read and cleared as the panel does.

use std::time::Duration;

use futures::executor::block_on;

use crate::{Chrome, Page, Profile, Viewport, find, test_server::serve};

const PAGE: &str = "<!doctype html><title>Keep</title><script>
document.cookie = 'theme=dark; path=/';
localStorage.setItem('draft', 'hello');
sessionStorage.setItem('tab', '2');
const open = indexedDB.open('notes', 1);
open.onupgradeneeded = () => open.result.createObjectStore('items', {keyPath: 'id'});
open.onsuccess = () => {
  const done = open.result.transaction('items', 'readwrite');
  done.objectStore('items').put({id: 1, text: 'first'});
  done.oncomplete = () => { document.title = 'Kept'; };
};
</script>";

async fn read_all(page: &Page, origin: &str) {
    let cookies = page.cookies(&format!("{origin}/")).await.unwrap();
    assert_eq!(cookies.len(), 1);
    assert_eq!(
        (cookies[0].name.as_str(), cookies[0].value.as_str()),
        ("theme", "dark")
    );
    let local = page.web_storage(true).await.unwrap();
    assert_eq!(local, [("draft".to_owned(), "hello".to_owned())]);
    let session = page.web_storage(false).await.unwrap();
    assert_eq!(session, [("tab".to_owned(), "2".to_owned())]);
    let databases = page.databases().await.unwrap();
    assert_eq!(databases.len(), 1);
    assert_eq!(databases[0].stores, ["items"]);
    let entries = page.store_entries("notes", "items").await.unwrap();
    assert_eq!(
        entries.entries,
        [("1".to_owned(), r#"{"id":1,"text":"first"}"#.to_owned())]
    );
}

async fn clear_all(page: &Page, origin: &str) {
    page.remove_web_storage(false, "tab").await.unwrap();
    assert_eq!(page.web_storage(false).await.unwrap(), []);
    page.clear_site_data(origin).await.unwrap();
    assert_eq!(page.cookies(&format!("{origin}/")).await.unwrap(), []);
    assert_eq!(page.web_storage(true).await.unwrap(), []);
}

#[test]
fn what_a_page_keeps_is_read_and_cleared() {
    let Some(browser) = find() else {
        return;
    };
    let origin = serve(PAGE);
    let dir = tempfile::tempdir().unwrap();
    let profile = Profile::claim(&dir.path().join("profile")).unwrap();
    let chrome = Chrome::launch(&browser, &profile, 1.0).unwrap();
    let viewport = Viewport {
        width: 400,
        height: 300,
        phone: false,
    };
    let (page, _events) = block_on(chrome.open_page(viewport)).unwrap();
    block_on(page.navigate(&format!("{origin}/"))).unwrap();
    for _ in 0..200 {
        if block_on(page.evaluate("document.title")).unwrap() == "Kept" {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    block_on(read_all(&page, &origin));
    block_on(clear_all(&page, &origin));
}
