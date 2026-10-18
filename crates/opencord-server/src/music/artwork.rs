//! The artwork proxy (Phase 3 plan §15.4). Clients load artwork from this
//! server, never from YouTube or other sites: an artwork id stands for an
//! `artworkUrl` Lavalink gave, and only those are fetched — https, public
//! hosts, at most 5 MB in 5 s. The picture is decoded, made smaller and
//! encoded again (which drops its metadata), then kept on disk for 7 days,
//! 200 MB at most, the least recently used going first.

use std::collections::HashMap;
use std::io::Cursor;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use image::ImageReader;
use image::codecs::jpeg::JpegEncoder;
use reqwest::Url;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use super::queue::Queue;
use super::tracks::Track;
use crate::lavalink::lock;

/// The sizes clients ask for: list thumbnails and the cover.
pub const SIZES: [u32; 2] = [128, 512];
const FETCH_BYTES: usize = 5 * 1024 * 1024;
const FETCH_TIME: Duration = Duration::from_secs(5);
const REDIRECTS: usize = 3;
/// Larger pictures are refused before they are decoded.
const MAX_SIDE: u32 = 8192;
const DECODE_BYTES: u64 = 256 * 1024 * 1024;
const QUALITY: u8 = 85;
const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const CACHE_BYTES: u64 = 200 * 1024 * 1024;
/// A picture that could not be fetched is not tried again for this long.
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
/// Ids remembered at once; the least recently seen go first.
const KNOWN_MAX: usize = 100_000;
/// Fetches at once, for everyone.
const FETCHES: usize = 4;
/// Inside the server's data directory.
const DIR: &str = "music-artwork";

/// The id clients see for an artwork address; empty without one. The same
/// address always has the same id, so a picture is kept once.
pub fn id_of(url: &str) -> String {
    if url.is_empty() {
        return String::new();
    }
    let digest = Sha256::digest(format!("opencord-artwork:{url}").as_bytes());
    hex::encode(&digest[..16])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkError {
    /// No track this server knows has it, or the size is not one of
    /// [`SIZES`].
    Unknown,
    /// It could not be fetched or read; tried again later.
    Unavailable,
}

#[derive(Debug)]
struct Cached {
    bytes: u64,
    stored: SystemTime,
    used: Instant,
}

/// The artwork ids handed out, and the pictures kept on disk.
#[derive(Debug)]
pub struct Artwork {
    /// Id → (address, when it was last seen).
    known: Mutex<HashMap<String, (String, Instant)>>,
    failed: Mutex<HashMap<String, Instant>>,
    /// File name → what is on disk; read from the directory once.
    cached: Mutex<Option<HashMap<String, Cached>>>,
    fetches: Semaphore,
}

impl Default for Artwork {
    fn default() -> Self {
        Self {
            known: Mutex::default(),
            failed: Mutex::default(),
            cached: Mutex::default(),
            fetches: Semaphore::new(FETCHES),
        }
    }
}

impl Artwork {
    /// The track's artwork may be fetched from now on.
    pub fn remember(&self, track: &Track) {
        let url = &track.info.artwork_url;
        if url.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut known = lock(&self.known);
        if known.len() >= KNOWN_MAX && !known.contains_key(&id_of(url)) {
            let oldest = known
                .iter()
                .min_by_key(|(_, (_, seen))| *seen)
                .map(|(id, _)| id.clone());
            if let Some(oldest) = oldest {
                known.remove(&oldest);
            }
        }
        known.insert(id_of(url), (url.clone(), now));
    }

    /// Every track in a queue, its history included.
    pub fn remember_queue(&self, queue: &Queue) {
        for item in queue
            .now
            .iter()
            .chain(&queue.upcoming)
            .chain(&queue.history)
        {
            self.remember(&item.track);
        }
    }

    fn url(&self, id: &str) -> Option<String> {
        lock(&self.known).get(id).map(|(url, _)| url.clone())
    }

    /// The picture for `id` at `size`, as JPEG: from disk, else fetched.
    /// `allow_private` lets it come from any http or https address, as
    /// `[music] allow_unsafe_sources` does for tracks.
    pub async fn get(
        &self,
        data_dir: &Path,
        id: &str,
        size: u32,
        allow_private: bool,
    ) -> Result<Vec<u8>, ArtworkError> {
        if !SIZES.contains(&size) {
            return Err(ArtworkError::Unknown);
        }
        let url = self.url(id).ok_or(ArtworkError::Unknown)?;
        let dir = data_dir.join(DIR);
        if let Some(bytes) = self.cached(&dir, id, size).await {
            return Ok(bytes);
        }
        if lock(&self.failed)
            .get(id)
            .is_some_and(|at| at.elapsed() < RETRY_AFTER)
        {
            return Err(ArtworkError::Unavailable);
        }
        let _turn = self
            .fetches
            .acquire()
            .await
            .map_err(|_| ArtworkError::Unavailable)?;
        // Another request may have fetched it meanwhile.
        if let Some(bytes) = self.cached(&dir, id, size).await {
            return Ok(bytes);
        }
        let rendered = match fetch(&url, allow_private).await {
            Ok(original) => tokio::task::spawn_blocking(move || render(&original))
                .await
                .map_err(|error| error.to_string())
                .and_then(|rendered| rendered),
            Err(error) => Err(error),
        };
        let rendered = match rendered {
            Ok(rendered) => rendered,
            Err(error) => {
                tracing::debug!(%error, "music artwork could not be fetched");
                lock(&self.failed).insert(id.to_owned(), Instant::now());
                return Err(ArtworkError::Unavailable);
            }
        };
        lock(&self.failed).remove(id);
        let mut wanted = Vec::new();
        for (each, bytes) in rendered {
            if let Err(error) = self.store(&dir, &file_name(id, each), &bytes).await {
                tracing::warn!(%error, "music artwork could not be kept on disk");
            }
            if each == size {
                wanted = bytes;
            }
        }
        Ok(wanted)
    }

    /// A kept picture, unless it is older than 7 days.
    async fn cached(&self, dir: &Path, id: &str, size: u32) -> Option<Vec<u8>> {
        let name = file_name(id, size);
        {
            let mut cached = self.index(dir).await;
            let entry = cached.as_mut()?.get_mut(&name)?;
            if age(entry.stored) > KEEP_FOR {
                return None;
            }
            entry.used = Instant::now();
        }
        tokio::fs::read(dir.join(&name)).await.ok()
    }

    /// The index of what is on disk, read from the directory the first
    /// time; the least recently used are those stored longest ago until
    /// they are used again.
    async fn index(
        &self,
        dir: &Path,
    ) -> std::sync::MutexGuard<'_, Option<HashMap<String, Cached>>> {
        let ready = lock(&self.cached).is_some();
        if !ready {
            let found = read_dir(dir).await;
            let mut cached = lock(&self.cached);
            if cached.is_none() {
                *cached = Some(found);
            }
            return cached;
        }
        lock(&self.cached)
    }

    /// Writes a picture, then removes expired ones and the least recently
    /// used until the rest fit in 200 MB.
    async fn store(&self, dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
        tokio::fs::create_dir_all(dir).await?;
        let path = dir.join(name);
        let partial = path.with_extension("partial");
        tokio::fs::write(&partial, bytes).await?;
        tokio::fs::rename(&partial, &path).await?;
        let doomed = {
            let mut guard = self.index(dir).await;
            let cached = guard.get_or_insert_with(HashMap::new);
            cached.insert(
                name.to_owned(),
                Cached {
                    bytes: bytes.len() as u64,
                    stored: SystemTime::now(),
                    used: Instant::now(),
                },
            );
            evict(cached, name)
        };
        for name in doomed {
            let _ = tokio::fs::remove_file(dir.join(name)).await;
        }
        Ok(())
    }
}

fn file_name(id: &str, size: u32) -> String {
    format!("{id}-{size}.jpg")
}

fn age(stored: SystemTime) -> Duration {
    SystemTime::now()
        .duration_since(stored)
        .unwrap_or(Duration::ZERO)
}

/// What the directory holds; a missing directory holds nothing.
async fn read_dir(dir: &Path) -> HashMap<String, Cached> {
    let mut found = HashMap::new();
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return found;
    };
    let now = Instant::now();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        if !metadata.is_file() || !name.ends_with(".jpg") {
            continue;
        }
        let stored = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        found.insert(
            name,
            Cached {
                bytes: metadata.len(),
                stored,
                used: now.checked_sub(age(stored)).unwrap_or(now),
            },
        );
    }
    found
}

/// Takes expired pictures out of the index, then the least recently used
/// until the rest fit, never `keep`; returns the files to remove.
fn evict(cached: &mut HashMap<String, Cached>, keep: &str) -> Vec<String> {
    let mut doomed: Vec<String> = cached
        .iter()
        .filter(|(name, entry)| *name != keep && age(entry.stored) > KEEP_FOR)
        .map(|(name, _)| name.clone())
        .collect();
    for name in &doomed {
        cached.remove(name);
    }
    let mut total: u64 = cached.values().map(|entry| entry.bytes).sum();
    if total > CACHE_BYTES {
        let mut by_use: Vec<(Instant, String, u64)> = cached
            .iter()
            .filter(|(name, _)| *name != keep)
            .map(|(name, entry)| (entry.used, name.clone(), entry.bytes))
            .collect();
        by_use.sort();
        for (_, name, bytes) in by_use {
            if total <= CACHE_BYTES {
                break;
            }
            cached.remove(&name);
            total -= bytes;
            doomed.push(name);
        }
    }
    doomed
}

/// Whether an address is on the public internet: not this machine, a
/// private or shared network, link-local, multicast or reserved.
pub fn is_public(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_unspecified()
                || v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || a == 0
                || a >= 240
                // Shared address space (RFC 6598).
                || (a == 100 && b & 0xc0 == 64)
                // IETF protocol assignments.
                || (a == 192 && b == 0 && c == 0)
                // Benchmarking (RFC 2544).
                || (a == 198 && b & 0xfe == 18))
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            !(v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                // Documentation (RFC 3849).
                || (first == 0x2001 && v6.segments()[1] == 0x0db8))
        }
    }
}

/// Fetches a picture: https to public addresses only (any with
/// `allow_private`), following at most 3 redirects, each checked again,
/// and connecting to the addresses that were checked.
async fn fetch(url: &str, allow_private: bool) -> Result<Vec<u8>, String> {
    tokio::time::timeout(FETCH_TIME, fetch_checked(url, allow_private))
        .await
        .map_err(|_| "it took longer than 5 s".to_owned())?
}

async fn fetch_checked(url: &str, allow_private: bool) -> Result<Vec<u8>, String> {
    let mut url = Url::parse(url).map_err(|error| error.to_string())?;
    for _ in 0..=REDIRECTS {
        let addresses = check(&url, allow_private).await?;
        let host = url.host_str().unwrap_or_default().to_owned();
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(
                crate::tls::client_config(None).map_err(|error| error.to_string())?,
            )
            .user_agent(concat!("Opencord/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(&host, &addresses)
            .build()
            .map_err(|error| error.to_string())?;
        let mut response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or("a redirect without a location")?;
            url = url.join(location).map_err(|error| error.to_string())?;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("the site answered {}", response.status()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > FETCH_BYTES as u64)
        {
            return Err("it is larger than 5 MB".to_owned());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            body.extend_from_slice(&chunk);
            if body.len() > FETCH_BYTES {
                return Err("it is larger than 5 MB".to_owned());
            }
        }
        return Ok(body);
    }
    Err("too many redirects".to_owned())
}

/// The addresses `url` may be fetched from.
async fn check(url: &Url, allow_private: bool) -> Result<Vec<SocketAddr>, String> {
    let allowed_scheme = match url.scheme() {
        "https" => true,
        "http" => allow_private,
        _ => false,
    };
    if !allowed_scheme {
        return Err(format!("{} addresses are not fetched", url.scheme()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("addresses with a user name are not fetched".to_owned());
    }
    let host = url.host_str().ok_or("an address without a host")?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.trim_matches(['[', ']']), port))
        .await
        .map_err(|error| format!("{host} could not be looked up: {error}"))?
        .collect();
    if addresses.is_empty() {
        return Err(format!("{host} has no address"));
    }
    if !allow_private && !addresses.iter().all(|address| is_public(address.ip())) {
        return Err(format!("{host} is not a public host"));
    }
    Ok(addresses)
}

/// The picture at every size in [`SIZES`], as JPEG without metadata; never
/// larger than it was.
fn render(original: &[u8]) -> Result<Vec<(u32, Vec<u8>)>, String> {
    let mut reader = ImageReader::new(Cursor::new(original))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(DECODE_BYTES);
    reader.limits(limits);
    let picture = reader.decode().map_err(|error| error.to_string())?;
    SIZES
        .iter()
        .map(|&size| {
            let fitted = if picture.width() <= size && picture.height() <= size {
                picture.clone()
            } else {
                picture.thumbnail(size, size)
            };
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, QUALITY)
                .encode_image(&fitted.to_rgb8())
                .map_err(|error| error.to_string())?;
            Ok((size, jpeg))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let picture = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(picture)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn ids_stand_for_their_address() {
        assert_eq!(id_of(""), "");
        let id = id_of("https://i.ytimg.com/vi/x/hq.jpg");
        assert_eq!(id.len(), 32);
        assert_eq!(id, id_of("https://i.ytimg.com/vi/x/hq.jpg"));
        assert_ne!(id, id_of("https://i.ytimg.com/vi/y/hq.jpg"));
    }

    #[test]
    fn only_public_addresses_are_public() {
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "192.0.0.1",
            "198.18.0.1",
            "::1",
            "::",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ] {
            assert!(!is_public(private.parse().unwrap()), "{private}");
        }
        for public in ["142.250.74.46", "1.1.1.1", "2a00:1450:4001:82a::200e"] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
    }

    #[test]
    fn pictures_are_made_smaller_and_encoded_again() {
        let rendered = render(&png(1000, 500)).unwrap();
        let sizes: Vec<(u32, u32)> = rendered
            .iter()
            .map(|(_, jpeg)| {
                let picture = image::load_from_memory(jpeg).unwrap();
                (picture.width(), picture.height())
            })
            .collect();
        assert_eq!(sizes, [(128, 64), (512, 256)]);
        assert!(
            rendered
                .iter()
                .all(|(_, jpeg)| jpeg.starts_with(&[0xff, 0xd8]))
        );
        let small = render(&png(40, 40)).unwrap();
        assert_eq!(image::load_from_memory(&small[1].1).unwrap().width(), 40);
        assert!(render(b"not a picture").is_err());
    }

    #[tokio::test]
    async fn only_public_https_addresses_are_fetched() {
        for refused in [
            "http://example.com/a.jpg",
            "https://127.0.0.1/a.jpg",
            "https://[::1]/a.jpg",
            "https://localhost/a.jpg",
            "https://user:pw@example.com/a.jpg",
            "file:///etc/passwd",
        ] {
            assert!(
                check(&Url::parse(refused).unwrap(), false).await.is_err(),
                "{refused}"
            );
        }
        assert!(
            check(&Url::parse("http://127.0.0.1:9/a.jpg").unwrap(), true)
                .await
                .is_ok()
        );
    }

    #[test]
    fn the_least_recently_used_go_first_and_old_ones_expire() {
        let now = Instant::now();
        let entry = |megabytes: u64, used_ago: u64, stored_ago: u64| Cached {
            bytes: megabytes * 1024 * 1024,
            stored: SystemTime::now() - Duration::from_secs(stored_ago),
            used: now - Duration::from_secs(used_ago),
        };
        let mut cached = HashMap::from([
            ("old.jpg".to_owned(), entry(1, 1, 8 * 24 * 60 * 60)),
            ("a.jpg".to_owned(), entry(100, 300, 0)),
            ("b.jpg".to_owned(), entry(100, 200, 0)),
            ("new.jpg".to_owned(), entry(50, 0, 0)),
        ]);
        let mut doomed = evict(&mut cached, "new.jpg");
        doomed.sort();
        assert_eq!(doomed, ["a.jpg", "old.jpg"]);
        assert!(cached.contains_key("b.jpg") && cached.contains_key("new.jpg"));
    }
}
