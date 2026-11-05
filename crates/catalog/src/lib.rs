//! The MyInstants implementation of the online sound library.

mod parser;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::{CatalogPage, SoundCatalog};
use reqwest::header::CONTENT_TYPE;
use reqwest::redirect::Policy;
use reqwest::{Client, StatusCode, Url};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

static NEXT_DOWNLOAD: AtomicU64 = AtomicU64::new(0);

const BASE_URL: &str = "https://www.myinstants.com";
const SOUND_PATH: &str = "/media/sounds/";
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
/// The longest sound the site is trusted to send. Its own limit is far lower.
const MAX_SOUND_BYTES: u64 = 25 * 1024 * 1024;
/// A results page is a few hundred kilobytes.
const MAX_PAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

pub struct MyInstantsCatalog {
    http: Client,
    base_url: String,
}

impl MyInstantsCatalog {
    /// Fails when the system's TLS library cannot start, which the caller can report.
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_base_url(BASE_URL)
    }

    fn with_base_url(base_url: &str) -> Result<Self, reqwest::Error> {
        // The site's own address is the only one worth following to: a redirect to another
        // host, or from https to http, would defeat the check on the sound URLs.
        let home = Url::parse(base_url).expect("the catalog address is valid");
        let redirects = Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if same_origin(attempt.url(), &home) {
                attempt.follow()
            } else {
                attempt.error("the redirect leaves the site")
            }
        });
        let builder = Client::builder()
            .user_agent(BROWSER_USER_AGENT)
            .redirect(redirects)
            // A slow link may take long to send a big sound, as long as it keeps sending.
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30));
        Ok(Self {
            http: builder.build()?,
            base_url: base_url.to_owned(),
        })
    }

    async fn fetch_page(
        &self,
        path: &str,
        query: &[(&str, &str)],
        page: u32,
    ) -> Result<CatalogPage, Failure> {
        let no_connection = |_| Failure::text(TextKey::CatalogNoConnection);
        let mut response = self
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

        response = response.error_for_status().map_err(no_connection)?;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(no_connection)? {
            if body.len() + chunk.len() > MAX_PAGE_BYTES {
                return Err(Failure::text(TextKey::CatalogNoConnection));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(parser::parse(
            &String::from_utf8_lossy(&body),
            &self.base_url,
            page,
        ))
    }

    async fn save(
        &self,
        sound_url: &str,
        partial: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut response = self.http.get(sound_url).send().await?.error_for_status()?;
        let is_page = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|kind| kind.to_str().ok())
            .is_some_and(|kind| kind.starts_with("text/"));
        if is_page {
            return Err("the site sent a page instead of a sound".into());
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_SOUND_BYTES)
        {
            return Err("the sound is too big".into());
        }

        let mut target = File::create(partial).await?;
        let mut received = 0;
        while let Some(chunk) = response.chunk().await? {
            received += chunk.len() as u64;
            if received > MAX_SOUND_BYTES {
                return Err("the sound is too big".into());
            }
            target.write_all(&chunk).await?;
        }
        target.flush().await?;
        Ok(())
    }
}

fn same_origin(url: &Url, home: &Url) -> bool {
    url.scheme() == home.scheme()
        && url.host_str() == home.host_str()
        && url.port_or_known_default() == home.port_or_known_default()
}

#[async_trait]
impl SoundCatalog for MyInstantsCatalog {
    async fn search(&self, query: &str, page: u32) -> Result<CatalogPage, Failure> {
        self.fetch_page("/en/search/", &[("name", query)], page)
            .await
    }

    async fn trending(&self, region: &str, page: u32) -> Result<CatalogPage, Failure> {
        self.fetch_page(&format!("/en/index/{region}/"), &[], page)
            .await
    }

    async fn download(&self, sound_url: &str, destination: &str) -> Result<(), Failure> {
        if !self.accepts(sound_url) {
            return Err(Failure::because(
                TextKey::SoundDownloadFailed,
                "the address is not one of the site's sounds",
            ));
        }
        // Each download has its own file, so two of the same sound cannot corrupt each other.
        let partial = format!(
            "{destination}.{}.part",
            NEXT_DOWNLOAD.fetch_add(1, Ordering::Relaxed)
        );
        let saved = self
            .save(sound_url, &partial)
            .await
            .map_err(|error| Failure::because(TextKey::SoundDownloadFailed, error));
        let finished = match saved {
            Ok(()) => fs::rename(&partial, destination)
                .await
                .map_err(|error| Failure::because(TextKey::SoundDownloadFailed, error)),
            Err(failure) => Err(failure),
        };
        if finished.is_err() {
            // A broken transfer must not leave its half file behind.
            let _ = fs::remove_file(&partial).await;
        }
        finished
    }

    fn accepts(&self, sound_url: &str) -> bool {
        let (Ok(url), Ok(home)) = (Url::parse(sound_url), Url::parse(&self.base_url)) else {
            return false;
        };
        same_origin(&url, &home)
            && url.username().is_empty()
            && url.password().is_none()
            && url.path().starts_with(SOUND_PATH)
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
            .unwrap()
            .trending("br", 99)
            .await
            .unwrap();

        assert!(page.sounds.is_empty());
        assert!(!page.has_more);
    }

    #[tokio::test]
    async fn a_server_error_means_no_connection() {
        let (address, _) = serve("500 Internal Server Error", b"");

        let failure = MyInstantsCatalog::with_base_url(&address)
            .unwrap()
            .trending("br", 1)
            .await
            .unwrap_err();

        assert_eq!(failure, Failure::text(TextKey::CatalogNoConnection));
    }

    #[tokio::test]
    async fn the_query_is_escaped() {
        let (address, request) = serve("200 OK", include_bytes!("../tests/fixtures/results.html"));

        let page = MyInstantsCatalog::with_base_url(&address)
            .unwrap()
            .search("a&b=c d", 1)
            .await
            .unwrap();

        let request = request.join().unwrap();
        assert!(request.contains("name=a%26b%3Dc+d&page=1"), "{request}");
        assert_eq!(page.sounds.len(), 2);
    }

    #[tokio::test]
    async fn the_trending_sounds_come_from_the_page_of_the_country() {
        let (address, request) = serve("200 OK", include_bytes!("../tests/fixtures/results.html"));

        MyInstantsCatalog::with_base_url(&address)
            .unwrap()
            .trending("fr", 2)
            .await
            .unwrap();

        let request = request.join().unwrap();
        assert!(
            request.starts_with("GET /en/index/fr/?page=2 "),
            "{request}"
        );
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
            .unwrap()
            .download(
                &format!("{address}/media/sounds/sound.mp3"),
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
            .unwrap()
            .download(
                &format!("{address}/media/sounds/sound.mp3"),
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

    #[test]
    fn only_sounds_of_the_site_over_its_own_scheme_are_accepted() {
        let catalog = MyInstantsCatalog::new().unwrap();

        assert!(catalog.accepts("https://www.myinstants.com/media/sounds/a.mp3"));
        for foreign in [
            "http://www.myinstants.com/media/sounds/a.mp3",
            "https://evil.example/media/sounds/a.mp3",
            "https://www.myinstants.com.evil.example/media/sounds/a.mp3",
            "https://user@www.myinstants.com/media/sounds/a.mp3",
            "https://www.myinstants.com/en/search/",
            "https://www.myinstants.com/media/sounds/../../x",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(!catalog.accepts(foreign), "{foreign}");
        }
    }

    #[tokio::test]
    async fn a_foreign_address_is_not_downloaded() {
        let (address, _) = serve("200 OK", b"audio bytes");
        let destination = std::env::temp_dir().join("instantpad-catalog-foreign.mp3");

        let result = MyInstantsCatalog::new()
            .unwrap()
            .download(
                &format!("{address}/media/sounds/a.mp3"),
                destination.to_str().unwrap(),
            )
            .await;

        assert!(result.is_err());
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn a_page_instead_of_a_sound_is_refused_and_leaves_nothing() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).unwrap();
            let reply = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc";
            stream.write_all(reply.as_bytes()).unwrap();
        });
        let folder = std::env::temp_dir().join(format!("instantpad-page-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let destination = folder.join("sound.mp3");

        let result = MyInstantsCatalog::with_base_url(&address)
            .unwrap()
            .download(
                &format!("{address}/media/sounds/a.mp3"),
                destination.to_str().unwrap(),
            )
            .await;

        assert!(result.is_err());
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn a_transfer_cut_short_leaves_no_partial_file() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).unwrap();
            let reply = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort";
            stream.write_all(reply.as_bytes()).unwrap();
        });
        let folder = std::env::temp_dir().join(format!("instantpad-cut-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let destination = folder.join("sound.mp3");

        let result = MyInstantsCatalog::with_base_url(&address)
            .unwrap()
            .download(
                &format!("{address}/media/sounds/a.mp3"),
                destination.to_str().unwrap(),
            )
            .await;

        assert!(result.is_err());
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
