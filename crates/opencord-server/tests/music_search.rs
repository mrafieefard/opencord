//! Search, links and artwork, Phase 3 L4 (plan §15): every enabled source
//! searches with its own prefix, links and private addresses are refused,
//! clients can't hand the server an encoded track, and artwork comes only
//! through the server's proxy. Against the fake Lavalink.

mod common;

use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::routing::get;
use common::fake_lavalink::FakeLavalink;
use common::music::*;
use common::*;
use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Response;

fn search_request(channel_id: i64, query: &str, source: &str) -> Kind {
    Kind::MusicSearch(proto::MusicSearch {
        channel_id,
        query: query.to_owned(),
        source: source.to_owned(),
    })
}

fn results(response: Response) -> proto::MusicSearchResults {
    match response {
        Response::MusicSearchResults(results) => results,
        other => panic!("expected search results, got {other:?}"),
    }
}

/// A search that waits out the rate limit (10 per 10 s) when it hits it.
async fn search_waiting(client: &mut TestClient, request: Kind) -> Response {
    loop {
        match client.request(request.clone()).await {
            Response::Error(error) if code(&error) == proto::ErrorCode::RateLimited => {
                let wait = error.retry_after_ms.unwrap_or(1_000);
                tokio::time::sleep(std::time::Duration::from_millis(wait + 50)).await;
            }
            response => return response,
        }
    }
}

/// The identifier of the fake's latest /v4/loadtracks.
fn last_load(lavalink: &FakeLavalink) -> String {
    lavalink.fake.loads.lock().unwrap().last().cloned().unwrap()
}

#[tokio::test]
async fn every_enabled_source_searches_with_its_prefix() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    *lavalink.fake.sources.lock().unwrap() = [
        "youtube",
        "soundcloud",
        "bandcamp",
        "spotify",
        "applemusic",
        "deezer",
    ]
    .map(str::to_owned)
    .to_vec();
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    add_node(&mut owner, "Home", &lavalink).await;
    let voice = channel_id(&ready, "General");
    owner.ok(join_voice(voice)).await;

    for (source, prefix, manager) in [
        ("youtube_music", "ytmsearch:", "youtube"),
        ("youtube", "ytsearch:", "youtube"),
        ("soundcloud", "scsearch:", "soundcloud"),
        ("bandcamp", "bcsearch:", "bandcamp"),
        ("spotify", "spsearch:", "spotify"),
        ("applemusic", "amsearch:", "applemusic"),
        ("deezer", "dzsearch:", "deezer"),
    ] {
        let found =
            results(search_waiting(&mut owner, search_request(voice, "song", source)).await);
        assert_eq!(last_load(&lavalink), format!("{prefix}song"), "{source}");
        assert_eq!(found.results.len(), 3, "{source}");
        let track = found.results[0].track.as_ref().unwrap();
        assert_eq!(track.source, manager);
        assert!(!track.source_display_name.is_empty());
    }

    // Only SoundCloud on: other sources and their links are refused, and a
    // prefix typed into the query can't pick one.
    owner
        .ok(Kind::UpdateMusicSettings(proto::UpdateMusicSettings {
            enabled_sources: Some(proto::StringList {
                values: vec!["soundcloud".to_owned()],
            }),
            default_source: Some("soundcloud".to_owned()),
            ..Default::default()
        }))
        .await;
    for (query, source) in [("song", "youtube"), ("https://youtu.be/dQw4w9WgXcQ", "")] {
        let Response::Error(refused) =
            search_waiting(&mut owner, search_request(voice, query, source)).await
        else {
            panic!("{query} from {source:?} is not refused");
        };
        assert_eq!(code(&refused), proto::ErrorCode::MusicSourceNotAllowed);
    }
    results(search_waiting(&mut owner, search_request(voice, "ytsearch:song", "")).await);
    assert_eq!(last_load(&lavalink), "scsearch:ytsearch:song");
    let linked = results(
        search_waiting(
            &mut owner,
            search_request(voice, "https://soundcloud.com/artist/song", ""),
        )
        .await,
    );
    assert_eq!(
        linked.results[0].track.as_ref().unwrap().source,
        "soundcloud"
    );
}

#[tokio::test]
async fn links_to_other_sites_and_private_addresses_never_reach_lavalink() {
    let Room {
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    let loads_before = lavalink.fake.loads.lock().unwrap().len();
    for refused in [
        "http://www.youtube.com/watch?v=x",
        "https://192.168.1.10/a.mp3",
        "https://[fd00::1]/a.mp3",
        "https://localhost:2333/a.mp3",
        "https://nas.local/a.mp3",
        "https://example.com/a.mp3",
        "file:///etc/passwd",
        "https://user:secret@www.youtube.com/watch?v=x",
    ] {
        let error = owner.error(search_request(voice, refused, "")).await;
        assert_eq!(
            code(&error),
            proto::ErrorCode::MusicLinkNotAllowed,
            "{refused}"
        );
    }
    assert_eq!(lavalink.fake.loads.lock().unwrap().len(), loads_before);

    // A known site whose link Lavalink plays through its http source: not
    // offered, since it lets anyone make Lavalink fetch any address.
    let found = results(
        owner
            .ok(search_request(
                voice,
                "https://www.youtube.com/watch?v=as-http",
                "",
            ))
            .await,
    );
    assert!(found.results.is_empty());
}

#[tokio::test]
async fn clients_cannot_hand_the_server_an_encoded_track() {
    let Room {
        mut owner,
        mut member,
        voice,
        lavalink: _lavalink,
        ..
    } = room().await;
    let found = search(&mut owner, voice, "song").await;
    // How the fake encodes the first result.
    let encoded = "enc:youtube:song 1".to_owned();

    // Neither an encoded track nor a made-up id is a result.
    for made_up in [encoded.clone(), "QAAAjQIAJVJpY2sgQXN0bGV5".to_owned()] {
        let error = owner.error(enqueue(voice, vec![made_up])).await;
        assert_eq!(code(&error), proto::ErrorCode::NotFound);
    }
    // Results are their searcher's only.
    member.ok(join_voice(voice)).await;
    let error = member.error(enqueue(voice, found.clone())).await;
    assert_eq!(code(&error), proto::ErrorCode::NotFound);

    // Nothing the server sends carries an encoded track.
    let response = owner.ok(enqueue(voice, found)).await;
    assert!(!format!("{response:?}").contains("enc:"));
    let response = owner.ok(snapshot_request(voice)).await;
    assert!(!format!("{response:?}").contains("enc:"));
}

/// A picture host on this machine, counting its requests.
async fn picture_host() -> (String, Arc<AtomicUsize>) {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        800,
        600,
        image::Rgb([10, 120, 200]),
    ))
    .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
    .unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&hits);
    let app = Router::new().route(
        "/cover.png",
        get(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            let png = png.clone();
            async move { ([("content-type", "image/png")], png) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/cover.png", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, hits)
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn artwork_loads_through_the_server_and_is_kept() {
    // A picture host on this machine is a private one: allowed only with
    // unsafe sources, as tests and development need.
    let Room {
        server,
        mut owner,
        voice,
        lavalink,
        media_token,
        ..
    } = room_with(|config| config.music.allow_unsafe_sources = true).await;
    let (url, hits) = picture_host().await;
    *lavalink.fake.artwork.lock().unwrap() = Some(url.clone());

    let found = results(owner.ok(search_request(voice, "song", "")).await);
    assert!(!format!("{found:?}").contains(&url), "clients never see it");
    let artwork_id = found.results[0].track.as_ref().unwrap().artwork_id.clone();
    assert_eq!(artwork_id.len(), 32);

    let auth = bearer(&media_token);
    let path = format!("/media/music/artwork/{artwork_id}?size=512");
    let (status, jpeg) = server
        .send_http("GET", &path, &[("Authorization", &auth)], &[])
        .await;
    assert_eq!(status, 200);
    let picture = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).unwrap();
    assert_eq!((picture.width(), picture.height()), (512, 384));
    let small = format!("/media/music/artwork/{artwork_id}?size=128");
    let (status, jpeg) = server
        .send_http("GET", &small, &[("Authorization", &auth)], &[])
        .await;
    assert_eq!(status, 200);
    assert_eq!(image::load_from_memory(&jpeg).unwrap().width(), 128);
    assert_eq!(hits.load(Ordering::SeqCst), 1, "fetched once, then kept");

    // The now-playing card points at the same picture.
    owner
        .ok(enqueue(voice, vec![found.results[0].result_id.clone()]))
        .await;
    let card = owner
        .wait_for(|event| match event {
            proto::event::Kind::MusicNowPlaying(card) if !card.ended => Some(card.clone()),
            _ => None,
        })
        .await;
    assert_eq!(card.artwork_id, artwork_id);

    for (path, status) in [
        (format!("/media/music/artwork/{artwork_id}?size=64"), 404),
        (
            "/media/music/artwork/0123456789abcdef0123456789abcdef".to_owned(),
            404,
        ),
    ] {
        let (got, _) = server
            .send_http("GET", &path, &[("Authorization", &auth)], &[])
            .await;
        assert_eq!(got, status, "{path}");
    }
    let (status, _) = server.send_http("GET", &path, &[], &[]).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn artwork_from_a_private_address_is_not_fetched() {
    let Room {
        server,
        mut owner,
        voice,
        lavalink,
        media_token,
        ..
    } = room().await;
    let (url, hits) = picture_host().await;
    *lavalink.fake.artwork.lock().unwrap() = Some(url);
    let found = results(owner.ok(search_request(voice, "song", "")).await);
    let artwork_id = found.results[0].track.as_ref().unwrap().artwork_id.clone();

    let path = format!("/media/music/artwork/{artwork_id}?size=128");
    let (status, _) = server
        .send_http(
            "GET",
            &path,
            &[("Authorization", &bearer(&media_token))],
            &[],
        )
        .await;
    assert_eq!(status, 502);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

/// A real Lavalink and the internet: each source searches, a found track's
/// link loads again, and its artwork comes from the real site through the
/// proxy. Ignored unless given (`docs/music.md`) `OPENCORD_TEST_LAVALINK`,
/// its address like `http://127.0.0.1:2333`, and
/// `OPENCORD_TEST_LAVALINK_PASSWORD`; `OPENCORD_TEST_SEARCH_SOURCES` lists
/// the sources to try, `youtube_music,youtube` (the youtube-source
/// plugin) unless set.
#[tokio::test]
#[ignore = "needs a Lavalink and the internet: see docs/music.md"]
async fn a_real_lavalink_searches_and_its_artwork_comes_through_the_proxy() {
    let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let port: u16 = env("OPENCORD_TEST_LAVALINK")
        .rsplit(':')
        .next()
        .unwrap()
        .trim_end_matches('/')
        .parse()
        .unwrap();
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let created = nodes(
        owner
            .ok(Kind::CreateLavalinkNode(proto::CreateLavalinkNode {
                settings: Some(node_settings("Real", port)),
                password: env("OPENCORD_TEST_LAVALINK_PASSWORD"),
            }))
            .await,
    );
    let node_id = created.nodes[0].id;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let list = nodes(
            owner
                .ok(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
                    watch: false,
                }))
                .await,
        );
        if list
            .nodes
            .iter()
            .any(|node| node.id == node_id && node.health == proto::LavalinkHealth::Healthy as i32)
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "never healthy");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let voice = channel_id(&ready, "General");
    owner.ok(join_voice(voice)).await;
    let token = bearer(&ready.media_token.as_ref().unwrap().token);

    let sources = std::env::var("OPENCORD_TEST_SEARCH_SOURCES")
        .unwrap_or_else(|_| "youtube_music,youtube".to_owned());
    for source in sources.split(',') {
        let found = results(
            search_waiting(&mut owner, search_request(voice, "lofi hip hop", source)).await,
        );
        assert!(!found.results.is_empty(), "{source} found nothing");
        let track = found.results[0].track.clone().unwrap();
        assert_eq!(track.source, source.trim_end_matches("_music"));
        println!(
            "{source}: {} results, first '{}' by {} ({})",
            found.results.len(),
            track.title,
            track.artist,
            track.uri
        );
        let linked =
            results(search_waiting(&mut owner, search_request(voice, &track.uri, "")).await);
        assert_eq!(linked.results[0].track.as_ref().unwrap().title, track.title);

        let artwork_id = found
            .results
            .iter()
            .map(|result| result.track.as_ref().unwrap().artwork_id.clone())
            .find(|id| !id.is_empty())
            .unwrap_or_else(|| panic!("no {source} result has artwork"));
        let path = format!("/media/music/artwork/{artwork_id}?size=512");
        let (status, jpeg) = server
            .send_http("GET", &path, &[("Authorization", &token)], &[])
            .await;
        assert_eq!(status, 200, "{source} artwork");
        let picture = image::load_from_memory(&jpeg).unwrap();
        println!(
            "{source} artwork: {}x{}, {} bytes",
            picture.width(),
            picture.height(),
            jpeg.len()
        );
        assert!(picture.width() <= 512 && picture.height() <= 512);
    }
}
