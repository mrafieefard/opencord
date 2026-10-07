//! Phase 2 V1 acceptance: voicebots talking through a real server's voice
//! node.

use std::time::{Duration, Instant};

use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use opencord_voicebot::{VoiceSession, Voicebot};

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.voice.udp_port = 0;
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }
}

/// An owner and a member bot, both in the General voice channel.
struct Call {
    _server: TestServer,
    owner: Voicebot,
    owner_voice: VoiceSession,
    member: Voicebot,
    member_voice: VoiceSession,
}

async fn call() -> Call {
    let server = TestServer::start().await;
    let owner = Voicebot::connect(
        &server.address(),
        server.handle.claim_token.clone(),
        "Owner",
    )
    .await
    .unwrap();
    let invite = owner
        .client
        .create_invite(&owner.server_key, None, None)
        .await
        .unwrap();
    let member = Voicebot::connect(&invite.link, None, "Member")
        .await
        .unwrap();
    let general = owner.voice_channel("General").unwrap();
    let owner_voice = owner.join(general).await.unwrap();
    let member_voice = member.join(general).await.unwrap();
    Call {
        _server: server,
        owner,
        owner_voice,
        member,
        member_voice,
    }
}

/// The longest silence between `start` and the last arrival.
fn longest_gap(start: Instant, arrivals: &[Instant]) -> Duration {
    let mut times: Vec<Instant> = arrivals.iter().copied().filter(|at| *at >= start).collect();
    times.sort();
    times.insert(0, start);
    times
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .max()
        .unwrap_or_default()
}

#[tokio::test]
async fn two_voicebots_hear_each_other_through_the_embedded_node() {
    let call = call().await;
    let owner_id = call.owner.user_id();
    let member_id = call.member.user_id();

    call.owner_voice
        .play_tone(440.0, Duration::from_secs(1))
        .await
        .unwrap();
    call.member_voice
        .play_tone(660.0, Duration::from_secs(1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let member_heard = call.member_voice.heard();
    let owner_heard = call.owner_voice.heard();
    let from_owner = &member_heard[&owner_id];
    let from_member = &owner_heard[&member_id];
    assert!(from_owner.decoded >= 45, "{from_owner:?}");
    assert!(from_member.decoded >= 45, "{from_member:?}");
    assert!(from_owner.peak_dbfs > -20.0, "{from_owner:?}");
    assert!(!member_heard.contains_key(&member_id), "no echo");
}

#[tokio::test]
async fn a_server_mute_stops_forwarding_within_100_ms() {
    let call = call().await;
    let member_id = call.member.user_id();
    let talking = call.member_voice.play_tone(440.0, Duration::from_secs(2));
    call.owner_voice
        .wait_until(|view| view.heard.get(&member_id).is_some_and(|h| h.decoded >= 10))
        .await
        .unwrap();

    let muted_at = Instant::now();
    call.owner
        .client
        .server_mute(&call.owner.server_key, member_id, true)
        .await
        .unwrap();
    talking.await.unwrap();

    let last = call.owner_voice.heard()[&member_id].last.unwrap();
    assert!(
        last <= muted_at + Duration::from_millis(100),
        "audio kept coming {:?} after the mute",
        last.saturating_duration_since(muted_at)
    );
}

#[tokio::test]
async fn a_deafened_voicebot_receives_no_audio_packets() {
    let call = call().await;
    call.member.client.set_voice_self(None, Some(true));
    tokio::time::sleep(Duration::from_millis(200)).await;
    call.member_voice.clear_heard();

    call.owner_voice
        .play_tone(440.0, Duration::from_millis(600))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        call.member_voice.heard().is_empty(),
        "{:?}",
        call.member_voice.heard()
    );
}

#[tokio::test]
async fn a_voice_gateway_resume_does_not_interrupt_media() {
    let call = call().await;
    let member_id = call.member.user_id();
    let talking = call.member_voice.play_tone(440.0, Duration::from_secs(2));
    call.owner_voice
        .wait_until(|view| view.heard.get(&member_id).is_some_and(|h| h.decoded >= 10))
        .await
        .unwrap();

    let dropped_at = Instant::now();
    call.owner_voice.drop_gateway();
    talking.await.unwrap();

    let heard = &call.owner_voice.heard()[&member_id];
    let gap = longest_gap(dropped_at, &heard.arrivals);
    assert!(gap < Duration::from_millis(200), "a gap of {gap:?}");
    assert_eq!(
        call.owner_voice.connections(),
        1,
        "it resumed, not rejoined"
    );
    assert_eq!(call.owner_voice.closed(), None);
}
