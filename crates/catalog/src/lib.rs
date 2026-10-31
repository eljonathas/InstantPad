//! The MyInstants implementation of the online sound library.

mod parser;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::{CatalogPage, SoundCatalog};
use reqwest::{Client, StatusCode};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

static NEXT_DOWNLOAD: AtomicU64 = AtomicU64::new(0);

const BASE_URL: &str = "https://www.myinstants.com";
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

pub struct MyInstantsCatalog {
    http: Client,
    base_url: String,
}

impl MyInstantsCatalog {
    pub fn new() -> Self {
        Self::with_base_url(BASE_URL)
    }

    fn with_base_url(base_url: &str) -> Self {
        let http = Client::builder()
            .user_agent(BROWSER_USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("the HTTP client settings are valid");
        Self {
            http,
            base_url: base_url.to_owned(),
        }
    }

    async fn fetch_page(
        &self,
        path: &str,
        query: &[(&str, &str)],
        page: u32,
    ) -> Result<CatalogPage, Failure> {
        let no_connection = |_| Failure::text(TextKey::CatalogNoConnection);
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .query(query)
            .query(&[("page", page)])
            .send()
            .await
            .map_err(no_connection)?;

        // Past the last page the site answers 404 rather than an empty list.
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(CatalogPage {
                sounds: Vec::new(),
                has_more: false,
            });
        }

        let html = response
            .error_for_status()
            .map_err(no_connection)?
            .text()
            .await
            .map_err(no_connection)?;
        Ok(parser::parse(&html, &self.base_url, page))
    }

    async fn save(&self, sound_url: &str, partial: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut response = self.http.get(sound_url).send().await?.error_for_status()?;
        let mut target = File::create(partial).await?;
        while let Some(chunk) = response.chunk().await? {
            target.write_all(&chunk).await?;
        }
        target.flush().await?;
        Ok(())
    }
}

impl Default for MyInstantsCatalog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SoundCatalog for MyInstantsCatalog {
    async fn search(&self, query: &str, page: u32) -> Result<CatalogPage, Failure> {
        self.fetch_page("/en/search/", &[("name", query)], page)
            .await
    }

    async fn trending(&self, page: u32) -> Result<CatalogPage, Failure> {
        self.fetch_page("/en/index/br/", &[], page).await
    }

    async fn download(&self, sound_url: &str, destination: &str) -> Result<(), Failure> {
        // Each download has its own file, so two of the same sound cannot corrupt each other.
        let partial = format!(
            "{destination}.{}.part",
            NEXT_DOWNLOAD.fetch_add(1, Ordering::Relaxed)
        );
        self.save(sound_url, &partial)
            .await
            .map_err(|error| Failure::because(TextKey::SoundDownloadFailed, error))?;
        fs::rename(&partial, destination)
            .await
            .map_err(|error| Failure::because(TextKey::SoundDownloadFailed, error))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use instantpad_application::message::Message;

    use super::*;

    /// Answers one request with `status` and `body`, and returns the request line it received.
    fn serve(status: &str, body: &'static [u8]) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let status = status.to_owned();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
            request.lines().next().unwrap().to_owned()
        });
        (address, handle)
    }

    #[tokio::test]
    async fn a_missing_page_is_an_empty_page_without_more() {
        let (address, _) = serve("404 Not Found", b"");

        let page = MyInstantsCatalog::with_base_url(&address)
            .trending(99)
            .await
            .unwrap();

        assert!(page.sounds.is_empty());
        assert!(!page.has_more);
    }

    #[tokio::test]
    async fn a_server_error_means_no_connection() {
        let (address, _) = serve("500 Internal Server Error", b"");

        let failure = MyInstantsCatalog::with_base_url(&address)
            .trending(1)
            .await
            .unwrap_err();

        assert_eq!(failure, Failure::text(TextKey::CatalogNoConnection));
    }

    #[tokio::test]
    async fn the_query_is_escaped() {
        let (address, request) = serve("200 OK", include_bytes!("../tests/fixtures/results.html"));

        let page = MyInstantsCatalog::with_base_url(&address)
            .search("a&b=c d", 1)
            .await
            .unwrap();

        let request = request.join().unwrap();
        assert!(request.contains("name=a%26b%3Dc+d&page=1"), "{request}");
        assert_eq!(page.sounds.len(), 2);
    }

    #[tokio::test]
    async fn a_download_lands_at_the_destination_and_leaves_no_partial_file() {
        let (address, _) = serve("200 OK", b"audio bytes");
        let folder =
            std::env::temp_dir().join(format!("instantpad-catalog-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let destination = folder.join("sound.mp3");
        std::fs::write(&destination, b"old").unwrap();

        MyInstantsCatalog::with_base_url(&address)
            .download(
                &format!("{address}/sound.mp3"),
                destination.to_str().unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"audio bytes");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn a_failed_download_says_why() {
        let (address, _) = serve("500 Internal Server Error", b"");
        let destination = std::env::temp_dir().join("instantpad-catalog-never-written.mp3");

        let failure = MyInstantsCatalog::with_base_url(&address)
            .download(
                &format!("{address}/sound.mp3"),
                destination.to_str().unwrap(),
            )
            .await
            .unwrap_err();

        let Failure(Message::Text { key, .. }) = failure else {
            panic!("expected a translated message");
        };
        assert_eq!(key, TextKey::SoundDownloadFailed);
        assert!(!destination.exists());
    }
}
