//! Talking to one Lavalink node (Phase 3 plan §12.2): its REST API and its
//! WebSocket, with the password in `Authorization`, and every failure put
//! in words an admin can act on.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::ServerName;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{self, Message};

use super::health::{FrameStats, Load};

/// Every REST call and connection gives up after this long.
pub const TIMEOUT: Duration = Duration::from_secs(10);
const CLIENT_NAME: &str = concat!("Opencord/", env!("CARGO_PKG_VERSION"));

/// Where a node is and how to sign in.
#[derive(Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub secure: bool,
    pub password: String,
    /// The internal Lavalink user id (plan §12.3).
    pub user_id: i64,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("secure", &self.secure)
            .finish_non_exhaustive()
    }
}

impl Endpoint {
    /// `host:port`, with brackets around an IPv6 address.
    pub fn authority(&self) -> String {
        match self.host.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => format!("[{}]:{}", self.host, self.port),
            _ => format!("{}:{}", self.host, self.port),
        }
    }

    fn base(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        format!("{scheme}://{}", self.authority())
    }
}

/// Why a call failed: the password, or anything else, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    PasswordRejected,
    Other(String),
}

impl Failure {
    pub fn text(&self) -> String {
        match self {
            Self::PasswordRejected => "Lavalink refused the password".to_owned(),
            Self::Other(text) => text.clone(),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text())
    }
}

/// `GET /v4/info`, as far as Opencord uses it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    pub version: String,
    pub sources: Vec<String>,
    pub plugins: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawInfo {
    version: RawVersion,
    #[serde(default)]
    source_managers: Vec<String>,
    #[serde(default)]
    plugins: Vec<RawPlugin>,
}

#[derive(Deserialize)]
struct RawVersion {
    semver: String,
}

#[derive(Deserialize)]
struct RawPlugin {
    name: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStats {
    players: u32,
    playing_players: u32,
    cpu: RawCpu,
    #[serde(default)]
    frame_stats: Option<RawFrames>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCpu {
    system_load: f64,
}

#[derive(Deserialize)]
struct RawFrames {
    sent: i64,
    nulled: i64,
    deficit: i64,
}

impl From<RawStats> for Load {
    fn from(stats: RawStats) -> Self {
        Self {
            players: stats.players,
            playing_players: stats.playing_players,
            system_load: if stats.cpu.system_load.is_finite() {
                stats.cpu.system_load.clamp(0.0, 1.0)
            } else {
                0.0
            },
            frames: stats.frame_stats.map(|frames| FrameStats {
                sent: frames.sent,
                nulled: frames.nulled,
                deficit: frames.deficit,
            }),
        }
    }
}

/// The REST API of one node.
#[derive(Debug, Clone)]
pub struct Rest {
    http: reqwest::Client,
    endpoint: Endpoint,
}

impl Rest {
    pub fn new(http: reqwest::Client, endpoint: Endpoint) -> Self {
        Self { http, endpoint }
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Sends one call: JSON in and out, the password, the timeout.
    pub async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, Failure> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.endpoint.base()))
            .header("Authorization", &self.endpoint.password)
            .header("Client-Name", CLIENT_NAME)
            .timeout(TIMEOUT);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| Failure::Other(explain_request(&error, &self.endpoint)))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(Failure::PasswordRejected);
        }
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let text = response
            .text()
            .await
            .map_err(|error| Failure::Other(explain_request(&error, &self.endpoint)))?;
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|value| value["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| status.to_string());
            return Err(Failure::Other(
                if status == reqwest::StatusCode::NOT_FOUND && path.starts_with("/v4/info") {
                    format!(
                        "{} answers, but it is not Lavalink 4 or newer",
                        self.endpoint.authority()
                    )
                } else {
                    format!("Lavalink answered {path} with: {message}")
                },
            ));
        }
        serde_json::from_str(&text).map_err(|_| {
            Failure::Other(format!(
                "{} answers, but not like Lavalink does",
                self.endpoint.authority()
            ))
        })
    }

    pub async fn info(&self) -> Result<Info, Failure> {
        let value = self.call(reqwest::Method::GET, "/v4/info", None).await?;
        let raw: RawInfo = serde_json::from_value(value).map_err(|_| {
            Failure::Other(format!(
                "{} answers, but not like Lavalink 4 does",
                self.endpoint.authority()
            ))
        })?;
        Ok(Info {
            version: raw.version.semver,
            sources: raw.source_managers,
            plugins: raw
                .plugins
                .into_iter()
                .map(|plugin| format!("{} {}", plugin.name, plugin.version))
                .collect(),
        })
    }

    /// `GET /v4/stats`, and how long it took.
    pub async fn stats(&self) -> Result<(Load, Duration), Failure> {
        let started = Instant::now();
        let value = self.call(reqwest::Method::GET, "/v4/stats", None).await?;
        let took = started.elapsed();
        let raw: RawStats = serde_json::from_value(value)
            .map_err(|_| Failure::Other("Lavalink's stats could not be read".to_owned()))?;
        Ok((raw.into(), took))
    }

    /// Keeps the session's players for a minute after the WebSocket drops.
    pub async fn enable_resuming(&self, session_id: &str) -> Result<(), Failure> {
        let body = serde_json::json!({ "resuming": true, "timeout": 60 });
        self.call(
            reqwest::Method::PATCH,
            &format!("/v4/sessions/{session_id}"),
            Some(&body),
        )
        .await
        .map(drop)
    }

    pub async fn update_player(
        &self,
        session_id: &str,
        guild_id: i64,
        body: &Value,
    ) -> Result<Value, Failure> {
        self.call(
            reqwest::Method::PATCH,
            &format!("/v4/sessions/{session_id}/players/{guild_id}"),
            Some(body),
        )
        .await
    }

    pub async fn destroy_player(&self, session_id: &str, guild_id: i64) -> Result<(), Failure> {
        self.call(
            reqwest::Method::DELETE,
            &format!("/v4/sessions/{session_id}/players/{guild_id}"),
            None,
        )
        .await
        .map(drop)
    }

    /// A player, or `None` when the session has none for that guild.
    pub async fn player(&self, session_id: &str, guild_id: i64) -> Result<Option<Value>, Failure> {
        match self
            .call(
                reqwest::Method::GET,
                &format!("/v4/sessions/{session_id}/players/{guild_id}"),
                None,
            )
            .await
        {
            Ok(player) => Ok(Some(player)),
            Err(Failure::Other(text)) if text.contains("Player not found") => Ok(None),
            Err(failure) => Err(failure),
        }
    }

    /// The guilds the session has players for.
    pub async fn player_guilds(&self, session_id: &str) -> Result<Vec<i64>, Failure> {
        let players = self
            .call(
                reqwest::Method::GET,
                &format!("/v4/sessions/{session_id}/players"),
                None,
            )
            .await?;
        Ok(players
            .as_array()
            .map(|players| {
                players
                    .iter()
                    .filter_map(|player| player["guildId"].as_str()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// `GET /v4/loadtracks`: a track, a playlist, search results, nothing,
    /// or Lavalink's error.
    pub async fn load_tracks(&self, identifier: &str) -> Result<Value, Failure> {
        let mut query = url_encode(identifier);
        query.insert_str(0, "/v4/loadtracks?identifier=");
        self.call(reqwest::Method::GET, &query, None).await
    }
}

/// Percent-encodes a query value.
fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A request that never got an answer, in words.
fn explain_request(error: &reqwest::Error, endpoint: &Endpoint) -> String {
    let chain = error_chain(error);
    if error.is_timeout() {
        return format!(
            "{} did not answer within {} s",
            endpoint.authority(),
            TIMEOUT.as_secs()
        );
    }
    explain_transport(&chain, endpoint)
}

/// A connection or TLS failure, from its error chain, in words.
fn explain_transport(chain: &str, endpoint: &Endpoint) -> String {
    let lower = chain.to_ascii_lowercase();
    let authority = endpoint.authority();
    if lower.contains("dns error")
        || lower.contains("failed to lookup")
        || lower.contains("name or service not known")
        || lower.contains("no address associated")
    {
        format!("{} could not be found: check the host name", endpoint.host)
    } else if lower.contains("connection refused")
        // Windows: "the target machine actively refused it".
        || lower.contains("actively refused")
    {
        format!("Nothing is listening on {authority}: check the port, and that Lavalink is running")
    } else if lower.contains("timed out") {
        format!("{authority} did not answer within {} s", TIMEOUT.as_secs())
    } else if lower.contains("unreachable") {
        format!("{authority} cannot be reached from this server")
    } else if endpoint.secure
        && (lower.contains("invalidcontenttype")
            || lower.contains("corrupt message")
            || lower.contains("wrong version number")
            || lower.contains("invalid message"))
    {
        format!("{authority} does not use TLS: turn TLS off for this node")
    } else if endpoint.secure && lower.contains("certificate") {
        format!(
            "The node's certificate could not be verified ({}). It needs a certificate from \
             an authority; for a node on a private network, turn TLS off",
            short_reason(chain)
        )
    } else if !endpoint.secure
        && (lower.contains("connection closed")
            || lower.contains("connection reset")
            || lower.contains("incompletemessage")
            || lower.contains("invalid http"))
    {
        format!("{authority} closed the connection: if it uses TLS, turn TLS on for this node")
    } else {
        format!("{authority} could not be reached: {}", short_reason(chain))
    }
}

fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        text.push_str(": ");
        text.push_str(&error.to_string());
        source = error.source();
    }
    text
}

/// The innermost reason of a chain, which says the most.
fn short_reason(chain: &str) -> String {
    chain.rsplit(": ").next().unwrap_or(chain).to_owned()
}

pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

pub type Socket = WebSocketStream<Box<dyn Io>>;

/// A WebSocket that said `ready`.
pub struct Connected {
    pub socket: Socket,
    pub session_id: String,
    pub resumed: bool,
}

/// Opens `/v4/websocket` and waits for `ready`. With `resume`, asks for
/// that session back.
pub async fn connect(endpoint: &Endpoint, resume: Option<&str>) -> Result<Connected, Failure> {
    tokio::time::timeout(TIMEOUT, connect_inner(endpoint, resume))
        .await
        .unwrap_or_else(|_| {
            Err(Failure::Other(format!(
                "{} did not finish the WebSocket handshake within {} s",
                endpoint.authority(),
                TIMEOUT.as_secs()
            )))
        })
}

async fn connect_inner(endpoint: &Endpoint, resume: Option<&str>) -> Result<Connected, Failure> {
    let other = |text: String| Failure::Other(text);
    let scheme = if endpoint.secure { "wss" } else { "ws" };
    let mut request = format!("{scheme}://{}/v4/websocket", endpoint.authority())
        .into_client_request()
        .map_err(|_| other(format!("{} is not a valid address", endpoint.authority())))?;
    let header = |value: &str| {
        HeaderValue::from_str(value)
            .map_err(|_| other("the password has characters a header cannot carry".to_owned()))
    };
    let headers = request.headers_mut();
    headers.insert("Authorization", header(&endpoint.password)?);
    headers.insert("User-Id", header(&endpoint.user_id.to_string())?);
    headers.insert("Client-Name", header(CLIENT_NAME)?);
    if let Some(session) = resume {
        headers.insert("Session-Id", header(session)?);
    }
    let tcp = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .await
        .map_err(|error| other(explain_transport(&error_chain(&error), endpoint)))?;
    let _ = tcp.set_nodelay(true);
    let stream: Box<dyn Io> = if endpoint.secure {
        let config = crate::tls::client_config(None)
            .map_err(|error| other(format!("TLS could not be set up: {error}")))?;
        let name = ServerName::try_from(endpoint.host.clone())
            .map_err(|_| other(format!("{} is not a valid host name", endpoint.host)))?;
        let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
            .connect(name, tcp)
            .await
            .map_err(|error| other(explain_transport(&error_chain(&error), endpoint)))?;
        Box::new(tls)
    } else {
        Box::new(tcp)
    };
    let (mut socket, _) = tokio_tungstenite::client_async(request, stream)
        .await
        .map_err(|error| match &error {
            tungstenite::Error::Http(response)
                if matches!(response.status().as_u16(), 401 | 403) =>
            {
                Failure::PasswordRejected
            }
            tungstenite::Error::Http(response) => other(format!(
                "{} refused the WebSocket ({})",
                endpoint.authority(),
                response.status()
            )),
            _ => other(explain_transport(&error_chain(&error), endpoint)),
        })?;
    loop {
        let Some(message) = socket.next().await else {
            return Err(other(
                "Lavalink closed the WebSocket before it was ready".to_owned(),
            ));
        };
        let message =
            message.map_err(|error| other(explain_transport(&error_chain(&error), endpoint)))?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
        if value["op"] == "ready" {
            let session_id = value["sessionId"]
                .as_str()
                .ok_or_else(|| other("Lavalink's ready had no session".to_owned()))?
                .to_owned();
            let resumed = value["resumed"].as_bool().unwrap_or(false);
            return Ok(Connected {
                socket,
                session_id,
                resumed,
            });
        }
    }
}

/// What came over a node's WebSocket.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Stats(Load),
    /// A player's event or position.
    Player {
        guild_id: i64,
        value: Value,
    },
    Other,
}

pub fn parse_op(text: &str) -> Op {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Op::Other;
    };
    let guild_id = || {
        value["guildId"]
            .as_str()
            .and_then(|id| id.parse::<i64>().ok())
    };
    match value["op"].as_str() {
        Some("stats") => serde_json::from_value::<RawStats>(value.clone())
            .map(|raw| Op::Stats(raw.into()))
            .unwrap_or(Op::Other),
        Some("event" | "playerUpdate") => match guild_id() {
            Some(guild_id) => Op::Player { guild_id, value },
            None => Op::Other,
        },
        _ => Op::Other,
    }
}

/// Sends a WebSocket ping; Lavalink's pong shows the connection is alive.
pub async fn ping(socket: &mut futures_util::stream::SplitSink<Socket, Message>) -> bool {
    socket.send(Message::Ping(Vec::new().into())).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(secure: bool) -> Endpoint {
        Endpoint {
            host: "lava.example.com".to_owned(),
            port: 2333,
            secure,
            password: "secret".to_owned(),
            user_id: 1,
        }
    }

    #[test]
    fn transport_errors_are_put_in_words() {
        let plain = endpoint(false);
        let tls = endpoint(true);
        assert_eq!(
            explain_transport(
                "error sending request: dns error: failed to lookup address",
                &plain
            ),
            "lava.example.com could not be found: check the host name"
        );
        assert!(
            explain_transport(
                "tcp connect error: Connection refused (os error 111)",
                &plain
            )
            .starts_with("Nothing is listening on lava.example.com:2333")
        );
        assert_eq!(
            explain_transport("received corrupt message of type InvalidContentType", &tls),
            "lava.example.com:2333 does not use TLS: turn TLS off for this node"
        );
        assert!(
            explain_transport("invalid peer certificate: UnknownIssuer", &tls)
                .contains("certificate could not be verified (UnknownIssuer)")
        );
        assert!(
            explain_transport("connection closed before message completed", &plain)
                .contains("turn TLS on")
        );
    }

    #[test]
    fn ipv6_hosts_get_brackets() {
        let mut endpoint = endpoint(false);
        endpoint.host = "2001:db8::1".to_owned();
        assert_eq!(endpoint.authority(), "[2001:db8::1]:2333");
    }

    #[test]
    fn the_password_stays_out_of_debug_output() {
        assert!(!format!("{:?}", endpoint(true)).contains("secret"));
    }

    #[test]
    fn ops_are_read() {
        let stats = r#"{"op":"stats","players":3,"playingPlayers":2,"uptime":1,
            "memory":{"free":1,"used":1,"allocated":1,"reservable":1},
            "cpu":{"cores":4,"systemLoad":0.5,"lavalinkLoad":0.1},
            "frameStats":{"sent":6000,"nulled":10,"deficit":-5}}"#;
        let Op::Stats(load) = parse_op(stats) else {
            panic!("not stats");
        };
        assert_eq!(load.players, 3);
        assert_eq!(load.playing_players, 2);
        assert!((load.system_load - 0.5).abs() < f64::EPSILON);
        assert_eq!(load.frames.unwrap().deficit, -5);
        let event = r#"{"op":"event","type":"WebSocketClosedEvent","guildId":"42","code":4006}"#;
        assert!(matches!(parse_op(event), Op::Player { guild_id: 42, .. }));
        assert_eq!(parse_op(r#"{"op":"ready"}"#), Op::Other);
    }
}
