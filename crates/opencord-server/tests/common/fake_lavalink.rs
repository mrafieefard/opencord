//! A stand-in Lavalink 4 node: its REST API and WebSocket as far as Opencord
//! uses them (sessions, resuming, stats, track loading, players and their
//! events), and, for a player with a voice state, Lavalink's voice client:
//! it signs in at the audio bridge and does UDP IP discovery, as Koe does.
//!
//! Searches find three tracks named after the query; a query with
//! "playlist" loads a playlist of 30, "live" a stream, and "broken" tracks
//! that fail to play.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch};
use futures_util::{SinkExt, StreamExt};
use opencord_common::address::Fingerprint;
use rustls::pki_types::ServerName;
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Notify;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// What the fake's voice client does with a player's voice state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// Nothing: as if it could not reach the bridge.
    Silent,
    /// Signs in, but no UDP gets through.
    SignInOnly,
    /// Signs in and does IP discovery.
    Discover,
}

pub struct Fake {
    pub password: String,
    pub version: String,
    pub sources: Mutex<Vec<String>>,
    /// playingPlayers and cpu.systemLoad, as stats report them.
    pub load: Mutex<(u32, f64)>,
    pub voice: Mutex<Voice>,
    /// The server's certificate, which the voice client pins.
    pub bridge_fingerprint: Mutex<Option<Fingerprint>>,
    /// Each WebSocket's handshake headers.
    pub handshakes: Mutex<Vec<HeaderMap>>,
    /// Bodies of PATCH /v4/sessions/{id}.
    pub session_updates: Mutex<Vec<Value>>,
    /// Bodies of player updates, by guild.
    pub players: Mutex<Vec<(i64, Value)>>,
    pub destroyed: Mutex<Vec<i64>>,
    /// Each guild's player now.
    pub live: Mutex<HashMap<i64, LivePlayer>>,
    /// Identifiers of every /v4/loadtracks.
    pub loads: Mutex<Vec<String>>,
    /// To the WebSocket: events and player updates.
    events: broadcast::Sender<String>,
    sessions: AtomicU32,
    current: Mutex<Option<String>>,
    /// Closes every WebSocket.
    kick: Notify,
}

/// Tracks every fake handed out, by their encoding: like Lavalink's,
/// encoded tracks play on any node.
static TRACKS: LazyLock<Mutex<HashMap<String, Value>>> = LazyLock::new(Mutex::default);

/// A player as the fake keeps it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LivePlayer {
    /// The track playing, as Lavalink sends it with its userData.
    pub track: Option<Value>,
    pub paused: bool,
    pub position: u64,
    pub voice: Option<Value>,
}

impl LivePlayer {
    pub fn encoded(&self) -> Option<String> {
        Some(self.track.as_ref()?["encoded"].as_str()?.to_owned())
    }

    pub fn title(&self) -> Option<String> {
        Some(self.track.as_ref()?["info"]["title"].as_str()?.to_owned())
    }
}

impl Fake {
    fn authorized(&self, headers: &HeaderMap) -> bool {
        headers
            .get("Authorization")
            .is_some_and(|value| value.as_bytes() == self.password.as_bytes())
    }

    /// Closes the WebSockets, as a network failure would.
    pub fn kick(&self) {
        self.kick.notify_waiters();
    }

    pub fn session(&self) -> Option<String> {
        self.current.lock().unwrap().clone()
    }

    pub fn set_load(&self, playing: u32, system_load: f64) {
        *self.load.lock().unwrap() = (playing, system_load);
    }

    fn send(&self, value: &Value) {
        let _ = self.events.send(value.to_string());
    }

    fn event(&self, guild: i64, kind: &str, track: &Value, extra: &Value) {
        let mut event =
            json!({ "op": "event", "type": kind, "guildId": guild.to_string(), "track": track });
        if let Some(extra) = extra.as_object() {
            event.as_object_mut().unwrap().extend(extra.clone());
        }
        self.send(&event);
    }

    pub fn player(&self, guild: i64) -> Option<LivePlayer> {
        self.live.lock().unwrap().get(&guild).cloned()
    }

    /// The guild's track plays to its end.
    pub fn finish(&self, guild: i64) {
        let track = self
            .live
            .lock()
            .unwrap()
            .get_mut(&guild)
            .and_then(|player| player.track.take());
        if let Some(track) = track {
            self.event(
                guild,
                "TrackEndEvent",
                &track,
                &json!({ "reason": "finished" }),
            );
        }
    }

    /// Reports the guild's position, as Lavalink does every few seconds.
    pub fn report_position(&self, guild: i64, position: u64) {
        if let Some(player) = self.live.lock().unwrap().get_mut(&guild) {
            player.position = position;
        }
        self.send(&json!({ "op": "playerUpdate", "guildId": guild.to_string(),
            "state": { "time": 0, "position": position, "connected": true, "ping": 1 } }));
    }

    fn player_json(&self, guild: i64, player: &LivePlayer) -> Value {
        json!({ "guildId": guild.to_string(), "track": player.track, "volume": 100,
            "paused": player.paused,
            "state": { "time": 0, "position": player.position, "connected": true, "ping": 1 },
            "voice": player.voice, "filters": {} })
    }

    fn make_track(&self, name: &str, source: &str, stream: bool) -> Value {
        let encoded = format!("enc:{source}:{name}");
        let track = json!({
            "encoded": encoded,
            "info": {
                "identifier": name, "isSeekable": !stream, "author": "Fake Artist",
                "length": if stream { 9_223_372_036_854_775_807u64 } else { 180_000 },
                "isStream": stream, "position": 0, "title": name,
                "uri": format!("https://example.com/{}", name.replace(' ', "-")),
                "artworkUrl": null, "isrc": null, "sourceName": source,
            },
            "pluginInfo": {},
            "userData": {},
        });
        TRACKS.lock().unwrap().insert(encoded, track.clone());
        track
    }
}

pub struct FakeLavalink {
    pub address: SocketAddr,
    pub fake: Arc<Fake>,
    task: JoinHandle<()>,
}

impl Drop for FakeLavalink {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl FakeLavalink {
    pub async fn start(password: &str) -> Self {
        let fake = Arc::new(Fake {
            password: password.to_owned(),
            version: "4.2.2".to_owned(),
            sources: Mutex::new(vec!["youtube".to_owned(), "soundcloud".to_owned()]),
            load: Mutex::new((0, 0.05)),
            voice: Mutex::new(Voice::Discover),
            bridge_fingerprint: Mutex::new(None),
            handshakes: Mutex::new(Vec::new()),
            session_updates: Mutex::new(Vec::new()),
            players: Mutex::new(Vec::new()),
            destroyed: Mutex::new(Vec::new()),
            live: Mutex::new(HashMap::new()),
            loads: Mutex::new(Vec::new()),
            events: broadcast::channel(256).0,
            sessions: AtomicU32::new(0),
            current: Mutex::new(None),
            kick: Notify::new(),
        });
        Self::serve(fake, "127.0.0.1:0".parse().unwrap()).await
    }

    async fn serve(fake: Arc<Fake>, address: SocketAddr) -> Self {
        let listener = TcpListener::bind(address).await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/v4/info", get(info))
            .route("/v4/stats", get(stats))
            .route("/v4/websocket", get(websocket))
            .route("/v4/loadtracks", get(load_tracks))
            .route("/v4/sessions/{session}", patch(update_session))
            .route("/v4/sessions/{session}/players", get(list_players))
            .route(
                "/v4/sessions/{session}/players/{guild}",
                get(get_player).patch(update_player).delete(destroy_player),
            )
            .with_state(Arc::clone(&fake));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            address,
            fake,
            task,
        }
    }

    pub fn port(&self) -> u16 {
        self.address.port()
    }

    /// Stops answering, as a crash would.
    pub fn stop(&self) {
        self.task.abort();
        self.fake.kick();
    }

    /// Starts again on the same port, with the same sessions.
    pub async fn restart(self) -> Self {
        self.stop();
        let fake = Arc::clone(&self.fake);
        let address = self.address;
        drop(self);
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self::serve(fake, address).await
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({ "status": 401, "error": "Unauthorized", "message": "Unauthorized" })),
    )
        .into_response()
}

async fn info(State(fake): State<Arc<Fake>>, headers: HeaderMap) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    axum::Json(json!({
        "version": { "semver": fake.version, "major": 4, "minor": 2, "patch": 2 },
        "buildTime": 0,
        "git": { "branch": "main", "commit": "0", "commitTime": 0 },
        "jvm": "17",
        "lavaplayer": "2",
        "sourceManagers": *fake.sources.lock().unwrap(),
        "filters": ["volume"],
        "plugins": [{ "name": "youtube-plugin", "version": "1.0.0" }],
    }))
    .into_response()
}

fn stats_json(fake: &Fake) -> Value {
    let (playing, load) = *fake.load.lock().unwrap();
    json!({
        "players": playing,
        "playingPlayers": playing,
        "uptime": 1000,
        "memory": { "free": 1, "used": 1, "allocated": 1, "reservable": 1 },
        "cpu": { "cores": 4, "systemLoad": load, "lavalinkLoad": load / 2.0 },
    })
}

async fn stats(State(fake): State<Arc<Fake>>, headers: HeaderMap) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    axum::Json(stats_json(&fake)).into_response()
}

async fn update_session(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Path(_session): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    fake.session_updates.lock().unwrap().push(body.clone());
    axum::Json(body).into_response()
}

async fn update_player(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Path((_session, guild)): Path<(String, i64)>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    fake.players.lock().unwrap().push((guild, body.clone()));
    let shown = apply_update(&fake, guild, &body);
    let voice = *fake.voice.lock().unwrap();
    let fingerprint = *fake.bridge_fingerprint.lock().unwrap();
    let user_id = fake
        .handshakes
        .lock()
        .unwrap()
        .last()
        .and_then(|headers| headers.get("User-Id")?.to_str().ok()?.parse::<i64>().ok())
        .unwrap_or(0);
    if voice != Voice::Silent
        && let (Some(state), Some(fingerprint)) = (body.get("voice"), fingerprint)
    {
        tokio::spawn(voice_client(
            state.clone(),
            guild,
            user_id,
            fingerprint,
            voice == Voice::Discover,
        ));
    }
    axum::Json(shown).into_response()
}

/// A player update: what plays, paused, position, voice; with the events
/// Lavalink sends for it.
fn apply_update(fake: &Fake, guild: i64, body: &Value) -> Value {
    let mut live = fake.live.lock().unwrap();
    let player = live.entry(guild).or_default();
    if let Some(voice) = body.get("voice") {
        player.voice = Some(voice.clone());
    }
    if let Some(paused) = body["paused"].as_bool() {
        player.paused = paused;
    }
    if let Some(position) = body["position"].as_u64() {
        player.position = position;
    }
    let mut events = Vec::new();
    if let Some(track) = body.get("track") {
        let encoded = track["encoded"].as_str();
        if let Some(old) = player.track.take() {
            let reason = if encoded.is_some() {
                "replaced"
            } else {
                "stopped"
            };
            events.push(("TrackEndEvent", old, json!({ "reason": reason })));
        }
        if let Some(encoded) = encoded {
            let mut full = TRACKS
                .lock()
                .unwrap()
                .get(encoded)
                .cloned()
                .unwrap_or_else(|| json!({ "encoded": encoded, "info": {} }));
            full["userData"] = track["userData"].clone();
            if encoded.contains("broken") {
                events.push((
                    "TrackExceptionEvent",
                    full.clone(),
                    json!({ "exception": { "message": "This video is unavailable",
                        "severity": "common", "cause": "x" } }),
                ));
                events.push(("TrackEndEvent", full, json!({ "reason": "loadFailed" })));
            } else {
                player.track = Some(full.clone());
                events.push(("TrackStartEvent", full, json!({})));
            }
        }
    }
    let shown = fake.player_json(guild, player);
    drop(live);
    for (kind, track, extra) in events {
        fake.event(guild, kind, &track, &extra);
    }
    shown
}

async fn get_player(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Path((_session, guild)): Path<(String, i64)>,
) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    match fake.player(guild) {
        Some(player) => axum::Json(fake.player_json(guild, &player)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "status": 404, "error": "Not Found",
                "message": "Player not found", "path": "/" })),
        )
            .into_response(),
    }
}

async fn list_players(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Path(_session): Path<String>,
) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    let players: Vec<Value> = fake
        .live
        .lock()
        .unwrap()
        .iter()
        .map(|(guild, player)| fake.player_json(*guild, player))
        .collect();
    axum::Json(players).into_response()
}

async fn load_tracks(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    let identifier = query.get("identifier").cloned().unwrap_or_default();
    fake.loads.lock().unwrap().push(identifier.clone());
    let source = |prefix: &str| match prefix {
        "scsearch" => "soundcloud",
        "http" | "https" => "http",
        _ => "youtube",
    };
    let (prefix, words) = identifier
        .split_once(':')
        .unwrap_or(("ytsearch", identifier.as_str()));
    let source = source(prefix);
    if prefix.starts_with("http") {
        let track = fake.make_track(&identifier, source, false);
        return axum::Json(json!({ "loadType": "track", "data": track })).into_response();
    }
    if words.contains("nothing") {
        return axum::Json(json!({ "loadType": "empty", "data": {} })).into_response();
    }
    if words.contains("playlist") {
        let tracks: Vec<Value> = (1..=30)
            .map(|n| fake.make_track(&format!("{words} {n}"), source, false))
            .collect();
        return axum::Json(json!({ "loadType": "playlist", "data": {
            "info": { "name": words, "selectedTrack": -1 }, "pluginInfo": {}, "tracks": tracks } }))
        .into_response();
    }
    let stream = words.contains("live");
    let tracks: Vec<Value> = (1..=3)
        .map(|n| fake.make_track(&format!("{words} {n}"), source, stream))
        .collect();
    axum::Json(json!({ "loadType": "search", "data": tracks })).into_response()
}

async fn destroy_player(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Path((_session, guild)): Path<(String, i64)>,
) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    fake.destroyed.lock().unwrap().push(guild);
    fake.live.lock().unwrap().remove(&guild);
    StatusCode::NO_CONTENT.into_response()
}

async fn websocket(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !fake.authorized(&headers) {
        return unauthorized();
    }
    let wanted = headers
        .get("Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    fake.handshakes.lock().unwrap().push(headers);
    upgrade.on_upgrade(move |socket| serve_socket(fake, socket, wanted))
}

async fn serve_socket(fake: Arc<Fake>, mut socket: WebSocket, wanted: Option<String>) {
    let current = fake.session();
    let (session, resumed) = match (wanted, current) {
        (Some(wanted), Some(current)) if wanted == current => (current, true),
        _ => {
            let number = fake.sessions.fetch_add(1, Ordering::Relaxed) + 1;
            (format!("fake-session-{number}"), false)
        }
    };
    *fake.current.lock().unwrap() = Some(session.clone());
    let ready = json!({ "op": "ready", "resumed": resumed, "sessionId": session });
    if socket
        .send(Message::Text(ready.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    let mut stats = json!({ "op": "stats" });
    stats
        .as_object_mut()
        .unwrap()
        .extend(stats_json(&fake).as_object().unwrap().clone());
    let _ = socket.send(Message::Text(stats.to_string().into())).await;
    let mut events = fake.events.subscribe();
    let kicked = fake.kick.notified();
    tokio::pin!(kicked);
    loop {
        tokio::select! {
            () = &mut kicked => return,
            event = events.recv() => match event {
                Ok(text) => {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
            message = socket.recv() => match message {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Lavalink's voice client, as far as the bridge sees it.
async fn voice_client(
    state: Value,
    guild: i64,
    user_id: i64,
    fingerprint: Fingerprint,
    discover: bool,
) {
    let endpoint = state["endpoint"].as_str().unwrap_or_default().to_owned();
    let Some((host, port)) = endpoint.rsplit_once(':') else {
        return;
    };
    let port: u16 = port.parse().unwrap();
    let Ok(tcp) = TcpStream::connect((host, port)).await else {
        return;
    };
    let config = opencord_server::tls::client_config(Some(fingerprint)).unwrap();
    let Ok(tls) = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from(host.to_owned()).unwrap(), tcp)
        .await
    else {
        return;
    };
    let request = format!("wss://{endpoint}/?v=8")
        .into_client_request()
        .unwrap();
    let Ok((mut socket, _)) = tokio_tungstenite::client_async(request, tls).await else {
        return;
    };
    use tokio_tungstenite::tungstenite::Message as Tm;
    // Hello first.
    match socket.next().await {
        Some(Ok(Tm::Text(_))) => {}
        _ => return,
    }
    let identify = json!({ "op": 0, "d": {
        "server_id": guild.to_string(),
        "user_id": user_id.to_string(),
        "session_id": state["sessionId"],
        "token": state["token"],
        "max_dave_protocol_version": 0,
    }});
    if socket
        .send(Tm::Text(identify.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    let ready = loop {
        match socket.next().await {
            Some(Ok(Tm::Text(text))) => {
                let value: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                if value["op"] == 2 {
                    break value;
                }
            }
            Some(Ok(_)) => {}
            _ => return,
        }
    };
    if discover {
        let ssrc = u32::try_from(ready["d"]["ssrc"].as_u64().unwrap()).unwrap();
        let port = u16::try_from(ready["d"]["port"].as_u64().unwrap()).unwrap();
        let ip: std::net::IpAddr = ready["d"]["ip"].as_str().unwrap().parse().unwrap();
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut request = [0u8; 74];
        request[..2].copy_from_slice(&1u16.to_be_bytes());
        request[2..4].copy_from_slice(&70u16.to_be_bytes());
        request[4..8].copy_from_slice(&ssrc.to_be_bytes());
        let _ = udp.send_to(&request, (ip, port)).await;
        let mut response = [0u8; 128];
        let _ = tokio::time::timeout(Duration::from_secs(2), udp.recv_from(&mut response)).await;
    }
    // Stays connected a while, as a player would.
    let _ = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(Ok(_)) = socket.next().await {}
    })
    .await;
}
