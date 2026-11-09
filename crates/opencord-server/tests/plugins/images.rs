// SPDX-License-Identifier: MPL-2.0

//! Images from a plugin's websites, fetched by the server for members
//! (plan M2 Task 11, design 2026-11-07 §13). The websites are one local
//! HTTPS site with a test authority's certificate; a test resolver points
//! public-looking names at it, which only the loopback policy lets through.

use std::collections::HashMap;
use std::io::{self, Cursor};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::{CONTENT_TYPE, HOST, LOCATION};
use axum::response::{IntoResponse, Response};
use axum_server::tls_rustls::RustlsConfig;
use futures_util::future::BoxFuture;
use opencord_plugins::limits::{IMAGE_CACHE_FRESH, IMAGE_CACHE_MAX_BYTES, IMAGE_FETCH_MAX_BYTES};
use opencord_proto::v1 as proto;
use opencord_server::plugins::images::{CACHE_FOLDER, TestReach};
use opencord_server::plugins::net::{Resolve, SystemResolver};
use opencord_server::testing::TestCa;
use proto::request::Kind;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::common::{
    TestClient, TestServer, decode_response, invite, join, media_token, owner, png,
};
use crate::support::{install, manifest_with, package, wait_for_log};

const PLUGIN: &str = "pictures";
const SERVER: &str = r#"opencord.on("start", function() log.info("pictures ready") end)"#;
/// Names the site answers for. The test resolver points the first two at
/// 127.0.0.1; `localhost` is left to the system's resolver.
const IMG: &str = "img.example.test";
const OTHER: &str = "other.example.test";
const LOCALHOST: &str = "localhost";
const MIB: usize = 1024 * 1024;
/// Eleven of these are more than the cache holds; ten fit.
const FILL: usize = IMAGE_CACHE_MAX_BYTES as usize / 10 - MIB / 2;
/// How long the slow image takes, so a second request arrives meanwhile.
const SLOW: Duration = Duration::from_millis(500);

fn encoded(format: image::ImageFormat) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([40, 90, 200, 255]));
    let mut bytes = Cursor::new(Vec::new());
    match format {
        // JPEG has no alpha.
        image::ImageFormat::Jpeg => image::DynamicImage::ImageRgba8(image)
            .to_rgb8()
            .write_to(&mut bytes, format)
            .unwrap(),
        _ => image.write_to(&mut bytes, format).unwrap(),
    }
    bytes.into_inner()
}

/// A GIF whose header says `width` × `height`, holding one 1×1 frame. Only
/// the header is read, so its size is what's checked.
fn gif(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    // A global table of two colours, white and black.
    bytes.extend_from_slice(&[0x80, 0, 0, 0xff, 0xff, 0xff, 0, 0, 0]);
    // One 1×1 frame of colour 0, and the end.
    bytes.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 0x01, 0, 0x3b]);
    bytes
}

/// A small PNG followed by zeros, `size` bytes in all: still a PNG to every
/// reader, which stops at its end.
fn padded_png(size: usize) -> Vec<u8> {
    let mut bytes = png(2, 2);
    bytes.resize(size, 0);
    bytes
}

/// Requests the site got, by `host` + path and query.
#[derive(Clone, Default)]
struct Hits(Arc<Mutex<HashMap<String, usize>>>);

impl Hits {
    fn add(&self, key: String) {
        *self.0.lock().unwrap().entry(key).or_default() += 1;
    }

    fn of(&self, host: &str, path: &str) -> usize {
        let hits = self.0.lock().unwrap();
        hits.get(&format!("{host}{path}")).copied().unwrap_or(0)
    }
}

async fn answer(State(hits): State<Hits>, request: Request) -> Response {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let (name, port) = host.split_once(':').unwrap_or((host.as_str(), "443"));
    let path = request
        .uri()
        .path_and_query()
        .map_or("/", |path| path.as_str());
    hits.add(format!("{name}{path}"));
    let image = |content_type: &'static str, bytes: Vec<u8>| {
        ([(CONTENT_TYPE, content_type)], bytes).into_response()
    };
    match request.uri().path() {
        "/a.png" => image("image/png", png(2, 2)),
        "/a.jpg" => image("image/jpeg", encoded(image::ImageFormat::Jpeg)),
        "/a.webp" => image("image/webp", encoded(image::ImageFormat::WebP)),
        "/a.gif" => image("image/gif", gif(1, 1)),
        "/slow.png" => {
            tokio::time::sleep(SLOW).await;
            image("image/png", png(2, 2))
        }
        "/logo.svg" => image(
            "image/svg+xml",
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#.to_vec(),
        ),
        "/page.png" => image(
            "image/png",
            b"<html><script>alert(1)</script></html>".to_vec(),
        ),
        "/40mp.gif" => image("image/gif", gif(8000, 5000)),
        "/over-40mp.gif" => image("image/gif", gif(8000, 5001)),
        "/10mib.png" => image("image/png", padded_png(IMAGE_FETCH_MAX_BYTES as usize)),
        "/big.png" => image("image/png", padded_png(IMAGE_FETCH_MAX_BYTES as usize + 1)),
        "/big-stream.png" => {
            // No length up front: the size shows only while reading.
            let bytes = padded_png(IMAGE_FETCH_MAX_BYTES as usize + 1);
            let pieces: Vec<Result<Bytes, io::Error>> = bytes
                .chunks(64 * 1024)
                .map(|piece| Ok(Bytes::copy_from_slice(piece)))
                .collect();
            let body = Body::from_stream(futures_util::stream::iter(pieces));
            ([(CONTENT_TYPE, "image/png")], body).into_response()
        }
        "/off-list" => (
            StatusCode::FOUND,
            [(LOCATION, format!("https://{OTHER}:{port}/a.png"))],
        )
            .into_response(),
        path if path.starts_with("/fill/") => image("image/png", padded_png(FILL)),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The local website.
struct Site {
    port: u16,
    hits: Hits,
    ca: TestCa,
}

impl Site {
    async fn start() -> Site {
        let ca = TestCa::new();
        let issued = ca.issue(&[IMG, OTHER, LOCALHOST]);
        let chain = vec![CertificateDer::from_pem_slice(issued.cert_pem.as_bytes()).unwrap()];
        let key = PrivateKeyDer::from_pem_slice(issued.key_pem.as_bytes()).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .unwrap();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let hits = Hits::default();
        let app = Router::new().fallback(answer).with_state(hits.clone());
        let handle = axum_server::Handle::<SocketAddr>::new();
        let server = axum_server::bind_rustls(
            "127.0.0.1:0".parse().unwrap(),
            RustlsConfig::from_config(Arc::new(config)),
        )
        .handle(handle.clone());
        tokio::spawn(async move { server.serve(app.into_make_service()).await });
        let port = handle.listening().await.unwrap().port();
        Site { port, hits, ca }
    }

    fn url(&self, host: &str, path: &str) -> String {
        format!("https://{host}:{}{path}", self.port)
    }

    fn hits(&self, host: &str, path: &str) -> usize {
        self.hits.of(host, path)
    }

    /// Fetches trusting the site's authority, looking names up with
    /// `resolver`; `loopback` lets them reach this machine.
    fn reach(&self, resolver: Arc<dyn Resolve>, loopback: bool) -> TestReach {
        TestReach {
            resolver,
            loopback,
            ca_der: self.ca.ca_der(),
        }
    }
}

/// Answers each name with its addresses.
struct Answers(HashMap<String, Vec<IpAddr>>);

impl Resolve for Answers {
    fn resolve(&self, host: &str) -> BoxFuture<'_, io::Result<Vec<IpAddr>>> {
        let found = self.0.get(host).cloned();
        Box::pin(async move { found.ok_or_else(|| io::Error::other("no such name")) })
    }
}

/// [`IMG`] and [`OTHER`] at 127.0.0.1.
fn local_names() -> Arc<dyn Resolve> {
    let here = vec![IpAddr::V4(Ipv4Addr::LOCALHOST)];
    Arc::new(Answers(HashMap::from([
        (IMG.to_owned(), here.clone()),
        (OTHER.to_owned(), here),
    ])))
}

/// A server fetching from `site` as the loopback policy allows, its owner,
/// and [`PLUGIN`] installed with `domains`, granted `granted`.
async fn pictures(
    site: &Site,
    domains: &[&str],
    granted: &[&str],
) -> (TestServer, TestClient, proto::Ready) {
    let server = TestServer::start().await;
    server
        .state()
        .plugins
        .images
        .reach_for_tests(site.reach(local_names(), true));
    let (mut owner, ready) = owner(&server).await;
    install_pictures(&server, &mut owner, &ready, domains, granted).await;
    (server, owner, ready)
}

async fn install_pictures(
    server: &TestServer,
    owner: &mut TestClient,
    ready: &proto::Ready,
    domains: &[&str],
    granted: &[&str],
) {
    let domains: Vec<String> = domains.iter().map(|domain| format!("{domain:?}")).collect();
    let extra = format!("domains = {{ {} }},", domains.join(", "));
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with(PLUGIN, "1.0.0", &["internet"], &extra),
        ),
        ("server/main.luau", SERVER),
    ]);
    install(server, owner, &media_token(ready), zip, granted).await;
    wait_for_log(owner, PLUGIN, "pictures ready").await;
}

/// `text` percent-encoded for a query string.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// Asks the server for the image at `url`, with `token` as the media
/// token: status, headers, body.
async fn image(
    server: &TestServer,
    token: Option<&str>,
    url: &str,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let path = format!("/plugins/{PLUGIN}/images?url={}", encode(url));
    let bearer = token.map(|token| format!("Bearer {token}"));
    let headers: Vec<(&str, &str)> = bearer
        .iter()
        .map(|bearer| ("Authorization", bearer.as_str()))
        .collect();
    server.send_http_full("GET", &path, &headers, b"").await
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// The error a refused request's body holds.
fn error_of(body: &[u8]) -> proto::Error {
    match decode_response(body) {
        proto::response::Result::Error(error) => error,
        other => panic!("not an error: {other:?}"),
    }
}

fn code_of(body: &[u8]) -> proto::ErrorCode {
    error_of(body).code()
}

/// The files in the plugin's image cache: names and sizes.
fn cached(server: &TestServer) -> Vec<(String, u64)> {
    let folder: PathBuf = server
        .dir
        .path()
        .join("data")
        .join("plugins")
        .join(PLUGIN)
        .join(CACHE_FOLDER);
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<(String, u64)> = entries
        .map(|entry| {
            let entry = entry.unwrap();
            let size = entry.metadata().unwrap().len();
            (entry.file_name().to_string_lossy().into_owned(), size)
        })
        .collect();
    files.sort();
    files
}

async fn member(server: &TestServer, owner: &mut TestClient, seed: u8) -> (TestClient, String) {
    let code = invite(owner, None).await;
    let (client, ready) = join(server, seed, &code).await;
    (client, media_token(&ready))
}

// Fails if two members' requests for one URL each fetch it (no waiting on
// the running fetch, or no cache), if the type or `nosniff` is missing, or
// if the route serves anyone without a media token.
#[tokio::test]
async fn an_allowed_png_is_served_and_cached() {
    let site = Site::start().await;
    let (server, mut owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let owner_token = media_token(&ready);
    let (_member, member_token) = member(&server, &mut owner, 2).await;
    let url = site.url(IMG, "/slow.png");

    let (first, second) = tokio::join!(
        image(&server, Some(&owner_token), &url),
        image(&server, Some(&member_token), &url)
    );
    for (status, headers, body) in [first, second] {
        assert_eq!(status, 200);
        assert_eq!(body, png(2, 2));
        assert_eq!(header(&headers, "content-type"), Some("image/png"));
        assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));
    }
    assert_eq!(site.hits(IMG, "/slow.png"), 1);

    let (status, headers, body) = image(&server, Some(&member_token), &url).await;
    assert_eq!((status, body), (200, png(2, 2)));
    let cache_control = header(&headers, "cache-control").unwrap();
    assert!(
        cache_control.starts_with("private, max-age="),
        "{cache_control}"
    );
    assert_eq!(site.hits(IMG, "/slow.png"), 1);
    assert_eq!(cached(&server).len(), 1);

    let (status, _, _) = image(&server, None, &url).await;
    assert_eq!(status, 401);
    assert_eq!(site.hits(IMG, "/slow.png"), 1);
}

// Fails if a cached image is served past an hour, or if fetching it again
// keeps a second copy.
#[tokio::test]
async fn a_stale_image_is_fetched_again() {
    let site = Site::start().await;
    let (server, _owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);
    let url = site.url(IMG, "/a.png");

    assert_eq!(image(&server, Some(&token), &url).await.0, 200);
    server
        .state()
        .plugins
        .images
        .move_clock(IMAGE_CACHE_FRESH - Duration::from_secs(60));
    assert_eq!(image(&server, Some(&token), &url).await.0, 200);
    assert_eq!(site.hits(IMG, "/a.png"), 1);

    server
        .state()
        .plugins
        .images
        .move_clock(Duration::from_secs(61));
    let (status, _, body) = image(&server, Some(&token), &url).await;
    assert_eq!((status, body), (200, png(2, 2)));
    assert_eq!(site.hits(IMG, "/a.png"), 2);
    assert_eq!(cached(&server).len(), 1);
}

// Fails if one of the four types is refused, or served as another type.
#[tokio::test]
async fn each_allowed_type_is_served_as_itself() {
    let site = Site::start().await;
    let (server, _owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);

    for (path, content_type) in [
        ("/a.png", "image/png"),
        ("/a.jpg", "image/jpeg"),
        ("/a.webp", "image/webp"),
        ("/a.gif", "image/gif"),
    ] {
        let (status, headers, _) = image(&server, Some(&token), &site.url(IMG, path)).await;
        assert_eq!(status, 200, "{path}");
        assert_eq!(
            header(&headers, "content-type"),
            Some(content_type),
            "{path}"
        );
    }
}

// Fails if the route fetches a host off the manifest's domains, an address
// written as one, or follows a redirect off them.
#[tokio::test]
async fn a_domain_off_the_list_is_refused() {
    let site = Site::start().await;
    let (server, _owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);

    let (status, _, body) = image(&server, Some(&token), &site.url(OTHER, "/a.png")).await;
    assert_eq!(status, 403);
    assert_eq!(
        error_of(&body).message,
        format!("{OTHER} isn't in the plugin's domains")
    );
    let (status, _, _) = image(&server, Some(&token), &site.url("127.0.0.1", "/a.png")).await;
    assert_eq!(status, 403);

    let (status, _, body) = image(&server, Some(&token), &site.url(IMG, "/off-list")).await;
    assert_eq!(status, 502);
    assert_eq!(code_of(&body), proto::ErrorCode::Internal);
    assert_eq!(site.hits(IMG, "/off-list"), 1);
    assert_eq!(site.hits(OTHER, "/a.png"), 0);
    assert_eq!(site.hits("127.0.0.1", "/a.png"), 0);
    assert!(cached(&server).is_empty());
}

// Fails if image fetches skip the address policy: with it, these names
// reach the site (which trusts its authority) and the image is served.
#[tokio::test]
async fn a_name_resolving_to_a_private_address_is_refused() {
    let site = Site::start().await;
    let (server, mut owner, ready) = pictures(&site, &[IMG, LOCALHOST], &["internet"]).await;
    let token = media_token(&ready);
    let images = &server.state().plugins.images;

    // The system's resolver, and the policy the server runs with.
    images.reach_for_tests(site.reach(Arc::new(SystemResolver), false));
    let (status, _, body) = image(&server, Some(&token), &site.url(LOCALHOST, "/a.png")).await;
    assert_eq!(status, 502);
    assert_eq!(error_of(&body).message, "the image couldn't be fetched");
    assert_eq!(site.hits(LOCALHOST, "/a.png"), 0);
    wait_for_log(
        &mut owner,
        PLUGIN,
        "localhost resolves only to addresses plugins may not reach",
    )
    .await;

    // A public-looking name answered with this machine's address.
    images.reach_for_tests(site.reach(local_names(), false));
    let (status, _, _) = image(&server, Some(&token), &site.url(IMG, "/a.png")).await;
    assert_eq!(status, 502);
    assert_eq!(site.hits(IMG, "/a.png"), 0);
    wait_for_log(
        &mut owner,
        PLUGIN,
        "img.example.test resolves only to addresses plugins may not reach",
    )
    .await;
    assert!(cached(&server).is_empty());
}

// Fails if a file is judged by its type header or name instead of its
// first bytes, or if a refused file stays in the cache's folder.
#[tokio::test]
async fn an_svg_is_refused() {
    let site = Site::start().await;
    let (server, _owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);

    for path in ["/logo.svg", "/page.png"] {
        let (status, headers, body) = image(&server, Some(&token), &site.url(IMG, path)).await;
        assert_eq!(status, 502, "{path}");
        assert_eq!(code_of(&body), proto::ErrorCode::FileTypeBlocked, "{path}");
        assert_ne!(header(&headers, "content-type"), Some("image/svg+xml"));
        assert!(!String::from_utf8_lossy(&body).contains("<script>"));
    }
    assert!(cached(&server).is_empty());
}

// Fails if the header's size isn't checked, or is checked against the
// wrong number (40 megapixels exactly is allowed).
#[tokio::test]
async fn too_many_pixels_is_refused() {
    let site = Site::start().await;
    let (server, _owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);

    let (status, _, body) = image(&server, Some(&token), &site.url(IMG, "/40mp.gif")).await;
    assert_eq!((status, body), (200, gif(8000, 5000)));

    let (status, _, body) = image(&server, Some(&token), &site.url(IMG, "/over-40mp.gif")).await;
    assert_eq!(status, 502);
    let error = error_of(&body);
    assert_eq!(error.code(), proto::ErrorCode::FileTooLarge);
    assert_eq!(error.message, "the image is larger than 40 megapixels");
    assert_eq!(cached(&server).len(), 1);
}

// Fails if an answer over 10 MiB is kept, whether its length is given up
// front or only shows while reading, or if 10 MiB exactly is refused.
#[tokio::test]
async fn over_10_mib_is_refused() {
    let site = Site::start().await;
    let (server, _owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);

    let (status, _, body) = image(&server, Some(&token), &site.url(IMG, "/10mib.png")).await;
    assert_eq!(status, 200);
    assert_eq!(body.len(), IMAGE_FETCH_MAX_BYTES as usize);

    for path in ["/big.png", "/big-stream.png"] {
        let (status, _, body) = image(&server, Some(&token), &site.url(IMG, path)).await;
        assert_eq!(status, 502, "{path}");
        let error = error_of(&body);
        assert_eq!(error.code(), proto::ErrorCode::FileTooLarge, "{path}");
        assert_eq!(error.message, "the image is larger than 10 MB");
    }
    // Only the 10 MiB image: nothing of the refused ones is left.
    assert_eq!(
        cached(&server)
            .into_iter()
            .map(|(_, size)| size)
            .collect::<Vec<_>>(),
        vec![IMAGE_FETCH_MAX_BYTES]
    );
}

// Fails if the route fetches for a plugin that wasn't granted `internet`,
// or for a plugin that isn't installed.
#[tokio::test]
async fn without_internet_the_route_refuses() {
    let site = Site::start().await;
    let (server, _owner, ready) = pictures(&site, &[IMG], &[]).await;
    let token = media_token(&ready);

    let (status, _, body) = image(&server, Some(&token), &site.url(IMG, "/a.png")).await;
    assert_eq!(status, 403);
    let error = error_of(&body);
    assert_eq!(error.code(), proto::ErrorCode::Forbidden);
    assert_eq!(error.message, "this plugin has no internet access");

    let path = format!(
        "/plugins/nobody/images?url={}",
        encode(&site.url(IMG, "/a.png"))
    );
    let bearer = format!("Bearer {token}");
    let (status, _, _) = server
        .send_http_full("GET", &path, &[("Authorization", &bearer)], b"")
        .await;
    assert_eq!(status, 404);
    assert_eq!(site.hits(IMG, "/a.png"), 0);
}

// Fails if the cache never pushes out its oldest image, or pushes out a
// newer one first.
#[tokio::test]
async fn the_cache_stays_under_its_cap() {
    let site = Site::start().await;
    let (server, _owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);

    for n in 0..11 {
        let path = format!("/fill/{n}");
        let (status, _, body) = image(&server, Some(&token), &site.url(IMG, &path)).await;
        assert_eq!((status, body.len()), (200, FILL), "{path}");
        let total: u64 = cached(&server).iter().map(|(_, size)| size).sum();
        assert!(total <= IMAGE_CACHE_MAX_BYTES, "{total} bytes after {path}");
    }
    assert_eq!(cached(&server).len(), 10);

    // The newest is still there; the oldest was fetched again.
    assert_eq!(
        image(&server, Some(&token), &site.url(IMG, "/fill/10"))
            .await
            .0,
        200
    );
    assert_eq!(site.hits(IMG, "/fill/10"), 1);
    assert_eq!(
        image(&server, Some(&token), &site.url(IMG, "/fill/0"))
            .await
            .0,
        200
    );
    assert_eq!(site.hits(IMG, "/fill/0"), 2);
}

// Fails if fetches aren't counted per member, if cached images are
// counted too, or if one member's limit stops another's.
#[tokio::test]
async fn uncached_fetches_are_rate_limited() {
    let site = Site::start().await;
    let (server, mut owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);
    let (_member, member_token) = member(&server, &mut owner, 2).await;

    for n in 0..20 {
        let url = site.url(IMG, &format!("/a.png?n={n}"));
        assert_eq!(image(&server, Some(&token), &url).await.0, 200, "fetch {n}");
    }
    let (status, _, body) = image(&server, Some(&token), &site.url(IMG, "/a.png?n=20")).await;
    assert_eq!(status, 429);
    assert_eq!(code_of(&body), proto::ErrorCode::RateLimited);
    assert_eq!(site.hits(IMG, "/a.png?n=20"), 0);

    let (status, _, _) = image(&server, Some(&token), &site.url(IMG, "/a.png?n=0")).await;
    assert_eq!(status, 200);
    assert_eq!(site.hits(IMG, "/a.png?n=0"), 1);
    let (status, _, _) = image(&server, Some(&member_token), &site.url(IMG, "/a.png?n=20")).await;
    assert_eq!(status, 200);
}

// Fails if uninstalling keeps the plugin's cached images, in memory (the
// image would be served again without a fetch) or on disk.
#[tokio::test]
async fn uninstalling_forgets_its_images() {
    let site = Site::start().await;
    let (server, mut owner, ready) = pictures(&site, &[IMG], &["internet"]).await;
    let token = media_token(&ready);
    let url = site.url(IMG, "/a.png");
    assert_eq!(image(&server, Some(&token), &url).await.0, 200);
    assert_eq!(cached(&server).len(), 1);

    owner
        .ok(Kind::UninstallPlugin(proto::UninstallPlugin {
            plugin_id: PLUGIN.to_owned(),
            delete_data: false,
        }))
        .await;
    assert!(cached(&server).is_empty());
    let plugin_folder = server.dir.path().join("data").join("plugins").join(PLUGIN);
    assert!(plugin_folder.exists(), "the plugin's data was kept");

    install_pictures(&server, &mut owner, &ready, &[IMG], &["internet"]).await;
    let (status, _, body) = image(&server, Some(&token), &url).await;
    assert_eq!((status, body), (200, png(2, 2)));
    assert_eq!(site.hits(IMG, "/a.png"), 2);
    assert_eq!(cached(&server).len(), 1);
}
