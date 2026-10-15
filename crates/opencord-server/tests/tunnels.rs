//! Cloudflare Tunnel, Phase 3 D4 (plan §9.2): cloudflared run, watched and
//! started again by the server, with a fake cloudflared (a shell script)
//! that prints what the real one does; and downloading cloudflared with
//! its checksum checked, from a fake release.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::routing::get;
use common::*;
use opencord_proto::v1 as proto;
use proto::request::Kind;
use serde_json::json;
use sha2::{Digest, Sha256};

const TOKEN: &str = "eyJhIjoiZmFrZS10dW5uZWwtdG9rZW4tZm9yLXRlc3RzIn0";
const QUICK_HOST: &str = "shy-fake-words.trycloudflare.com";

/// A fake cloudflared: `--version` answers; a run records its arguments
/// and pid in its HOME, prints `body`, and waits.
fn fake_cloudflared(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("cloudflared");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then echo 'cloudflared version 2099.1.0 (built test)'; exit 0; fi\n\
             echo \"$@\" > \"$HOME/args\"\n\
             echo $$ > \"$HOME/pid\"\n\
             {body}\n\
             exec sleep 600\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

const QUICK: &str = "echo 'INF Requesting new quick Tunnel on trycloudflare.com...' >&2\n\
     echo 'INF |  https://shy-fake-words.trycloudflare.com  |' >&2\n\
     echo 'INF Registered tunnel connection connIndex=0' >&2";

/// Says its token, fails on the first run, then stays up.
const NAMED: &str = "echo \"INF starting with $TUNNEL_TOKEN\" >&2\n\
     n=$(cat \"$HOME/runs\" 2>/dev/null || echo 0); n=$((n+1)); echo $n > \"$HOME/runs\"\n\
     if [ $n -lt 2 ]; then echo 'ERR Connection terminated error=\"boom\"' >&2; exit 1; fi\n\
     echo 'INF Registered tunnel connection connIndex=0' >&2";

async fn server_running(body: &str) -> (TestServer, tempfile::TempDir) {
    let bin = tempfile::tempdir().unwrap();
    let path = fake_cloudflared(bin.path(), body);
    let server = TestServer::start_with(|config| {
        config.tunnel.cloudflared = Some(path);
    })
    .await;
    // Any free port, so tests can run at once.
    server
        .handle
        .set_local_listener_on_any_port(false)
        .await
        .unwrap();
    (server, bin)
}

fn network(response: proto::response::Result) -> proto::NetworkInfo {
    match response {
        proto::response::Result::NetworkInfo(info) => *info,
        other => panic!("expected network info, got {other:?}"),
    }
}

/// The tunnel once `pick` finds what it waits for.
async fn tunnel_when(
    owner: &mut TestClient,
    pick: impl Fn(&proto::NetworkInfo) -> bool,
) -> proto::NetworkInfo {
    let found = owner.wait_for(|event| match event {
        proto::event::Kind::NetworkStatus(status) => {
            status.info.as_ref().filter(|info| pick(info)).cloned()
        }
        _ => None,
    });
    tokio::time::timeout(Duration::from_secs(10), found)
        .await
        .expect("the tunnel gets there")
}

fn state(info: &proto::NetworkInfo) -> proto::TunnelState {
    proto::TunnelState::try_from(info.tunnel.as_ref().unwrap().state).unwrap()
}

fn cloudflared_home(server: &TestServer) -> PathBuf {
    server.dir.path().join("data/cloudflared")
}

fn alive(pid: &str) -> bool {
    Path::new(&format!("/proc/{}", pid.trim())).exists()
}

#[tokio::test]
async fn a_quick_tunnel_gets_its_address_as_a_domain_and_takes_it_away() {
    let (server, _bin) = server_running(QUICK).await;
    let (mut owner, _) = owner(&server).await;
    let watched = network(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    assert_eq!(
        watched.tunnel.unwrap().cloudflared_version,
        "2099.1.0",
        "cloudflared is found"
    );

    owner
        .ok(Kind::StartTunnel(proto::StartTunnel {
            kind: proto::TunnelKind::Quick as i32,
            token: String::new(),
            hostname: String::new(),
        }))
        .await;
    let running = tunnel_when(&mut owner, |info| {
        state(info) == proto::TunnelState::Running
            && info.tunnel.as_ref().unwrap().hostname == QUICK_HOST
    })
    .await;

    let tunnel = running.tunnel.unwrap();
    let domain = running
        .domains
        .iter()
        .find(|domain| domain.hostname == QUICK_HOST)
        .expect("the address is a domain");
    assert_eq!(domain.kind, proto::DomainKind::Tunnel as i32);
    assert_eq!(tunnel.domain_id, domain.id);
    let local = server
        .handle
        .local_listener_port()
        .expect("the local listener runs");
    let args = std::fs::read_to_string(cloudflared_home(&server).join("args")).unwrap();
    assert_eq!(
        args.trim(),
        format!("tunnel --no-autoupdate --url http://127.0.0.1:{local}")
    );
    let pid = std::fs::read_to_string(cloudflared_home(&server).join("pid")).unwrap();
    assert!(alive(&pid));

    let stopped = network(owner.ok(Kind::StopTunnel(proto::StopTunnel {})).await);

    assert_eq!(state(&stopped), proto::TunnelState::Off);
    assert!(
        stopped.domains.is_empty(),
        "a quick tunnel's address dies with it"
    );
    for _ in 0..50 {
        if !alive(&pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!alive(&pid), "cloudflared is stopped");
}

#[tokio::test]
async fn a_named_tunnel_runs_with_its_token_hidden_and_starts_again_when_it_stops() {
    let (server, _bin) = server_running(NAMED).await;
    let (mut owner, _) = owner(&server).await;
    owner
        .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;

    let short = owner
        .error(Kind::StartTunnel(proto::StartTunnel {
            kind: proto::TunnelKind::Named as i32,
            token: "short".to_owned(),
            hostname: "chat.example.com".to_owned(),
        }))
        .await;
    assert_eq!(short.code, proto::ErrorCode::InvalidArgument as i32);
    let started = network(
        owner
            .ok(Kind::StartTunnel(proto::StartTunnel {
                kind: proto::TunnelKind::Named as i32,
                token: TOKEN.to_owned(),
                hostname: "Chat.Example.com".to_owned(),
            }))
            .await,
    );
    assert!(
        started
            .domains
            .iter()
            .any(|domain| domain.hostname == "chat.example.com"
                && domain.kind == proto::DomainKind::Tunnel as i32)
    );
    let restarting = tunnel_when(&mut owner, |info| {
        state(info) == proto::TunnelState::Restarting
    })
    .await;
    assert!(
        restarting.tunnel.unwrap().error.contains("boom"),
        "why it stopped"
    );
    let running = tunnel_when(&mut owner, |info| {
        state(info) == proto::TunnelState::Running
    })
    .await;

    let tunnel = running.tunnel.clone().unwrap();
    assert_eq!(tunnel.restarts, 1);
    assert_eq!(tunnel.hostname, "chat.example.com");
    assert!(tunnel.error.is_empty());
    let args = std::fs::read_to_string(cloudflared_home(&server).join("args")).unwrap();
    assert_eq!(args.trim(), "tunnel --no-autoupdate run");
    let log = match owner
        .ok(Kind::FetchTunnelLog(proto::FetchTunnelLog {}))
        .await
    {
        proto::response::Result::TunnelLog(log) => log.lines,
        other => panic!("expected the log, got {other:?}"),
    };
    assert!(log.iter().any(|line| line.contains("starting with ••••")));
    assert!(!log.iter().any(|line| line.contains(TOKEN)), "{log:?}");
    assert!(!format!("{running:?}").contains(TOKEN));

    let stopped = network(owner.ok(Kind::StopTunnel(proto::StopTunnel {})).await);
    assert_eq!(state(&stopped), proto::TunnelState::Off);
    assert_eq!(stopped.domains.len(), 1, "a named tunnel's domain stays");
}

#[tokio::test]
async fn a_tunnel_starts_again_with_the_server() {
    let (server, bin) = server_running(NAMED).await;
    let (mut owner, _) = owner(&server).await;
    // Saved, so the restarted server's listener does not take 7719.
    owner
        .ok(Kind::UpdateNetworkSettings(proto::UpdateNetworkSettings {
            local_listener_port: Some(u32::from(common::pebble::free_port())),
            ..Default::default()
        }))
        .await;
    owner
        .ok(Kind::StartTunnel(proto::StartTunnel {
            kind: proto::TunnelKind::Named as i32,
            token: TOKEN.to_owned(),
            hostname: "chat.example.com".to_owned(),
        }))
        .await;
    owner.close().await;
    let path = bin.path().join("cloudflared");

    let server = server
        .restart(|config| config.tunnel.cloudflared = Some(path))
        .await;
    let mut owner = TestClient::connect(&server, key(1)).await;
    owner.identify("Owner", None, None).await.unwrap();
    let watched = network(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    let running = if state(&watched) == proto::TunnelState::Running {
        watched
    } else {
        tunnel_when(&mut owner, |info| {
            state(info) == proto::TunnelState::Running
        })
        .await
    };

    let tunnel = running.tunnel.unwrap();
    assert_eq!(tunnel.kind, proto::TunnelKind::Named as i32);
    assert_eq!(tunnel.hostname, "chat.example.com");
    assert_eq!(running.domains.len(), 1, "the same domain, not another");
}

#[tokio::test]
async fn only_the_owner_runs_tunnels_and_none_runs_without_cloudflared() {
    let bin = tempfile::tempdir().unwrap();
    // There, but it cannot be run.
    let broken = bin.path().join("cloudflared");
    std::fs::write(&broken, "not a program").unwrap();
    let server = TestServer::start_with(|config| {
        config.tunnel.cloudflared = Some(broken);
    })
    .await;
    server
        .handle
        .set_local_listener_on_any_port(false)
        .await
        .unwrap();
    let missing = TestServer::start_with(|config| {
        config.tunnel.cloudflared = Some(PathBuf::from("/nonexistent/cloudflared"));
    })
    .await;
    let (mut without, _) = owner(&missing).await;
    let (mut owner, _) = owner(&server).await;
    owner
        .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;
    let code = invite(&mut owner, None).await;
    let (mut member, _) = join(&server, 2, &code).await;
    let quick = Kind::StartTunnel(proto::StartTunnel {
        kind: proto::TunnelKind::Quick as i32,
        token: String::new(),
        hostname: String::new(),
    });

    let not_installed = without.error(quick.clone()).await;
    let by_member = member.error(quick.clone()).await;
    let log_by_member = member
        .error(Kind::FetchTunnelLog(proto::FetchTunnelLog {}))
        .await;
    owner.ok(quick).await;
    let failed = tunnel_when(&mut owner, |info| state(info) == proto::TunnelState::Failed).await;

    assert!(
        not_installed.message.contains("not installed"),
        "{}",
        not_installed.message
    );
    assert_eq!(by_member.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(log_by_member.code, proto::ErrorCode::Forbidden as i32);
    assert!(
        failed.tunnel.unwrap().error.contains("could not start"),
        "it says why"
    );
}

/// GitHub's release description and the file, as far as Opencord reads
/// them.
async fn fake_release(file: Vec<u8>, listed: String) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let asset = format!(
        "cloudflared-linux-{}",
        match std::env::consts::ARCH {
            "aarch64" => "arm64",
            _ => "amd64",
        }
    );
    let release = json!({
        "tag_name": "2099.1.0",
        "body": format!("### SHA256 Checksums:\n```\n{asset}.deb: {}\n{asset}: {listed}\n```", "0".repeat(64)),
        "assets": [
            { "name": format!("{asset}.deb"), "browser_download_url": format!("http://{address}/nothing") },
            { "name": asset, "browser_download_url": format!("http://{address}/download") },
        ],
    });
    let app = axum::Router::new()
        .route("/release", get(move || async move { axum::Json(release) }))
        .route("/download", get(move || async move { file }));
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{address}/release")
}

fn fake_binary() -> Vec<u8> {
    b"#!/bin/sh\necho 'cloudflared version 2099.1.0 (built test)'\n".to_vec()
}

async fn install(server: &TestServer) -> proto::TunnelStatus {
    let (mut owner, _) = owner(server).await;
    owner
        .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;
    owner
        .ok(Kind::InstallCloudflared(proto::InstallCloudflared {}))
        .await;
    tunnel_when(&mut owner, |info| !info.tunnel.as_ref().unwrap().installing)
        .await
        .tunnel
        .unwrap()
}

#[tokio::test]
async fn cloudflared_is_downloaded_only_when_its_checksum_matches() {
    let good = fake_binary();
    let api = fake_release(good.clone(), hex::encode(Sha256::digest(&good))).await;
    let server = TestServer::start_with(|config| {
        config.tunnel.cloudflared = None;
        config.tunnel.releases_api = api;
    })
    .await;

    let status = install(&server).await;

    assert_eq!(status.cloudflared_version, "2099.1.0", "{}", status.error);
    let installed = server.dir.path().join("data/bin/cloudflared");
    assert_eq!(std::fs::read(&installed).unwrap(), good);
    assert_eq!(
        std::fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[tokio::test]
async fn a_download_with_the_wrong_checksum_is_thrown_away() {
    let api = fake_release(fake_binary(), "ab".repeat(32)).await;
    let server = TestServer::start_with(|config| {
        config.tunnel.cloudflared = None;
        config.tunnel.releases_api = api;
    })
    .await;

    let status = install(&server).await;

    assert!(status.error.contains("checksum"), "{}", status.error);
    assert!(!server.dir.path().join("data/bin/cloudflared").exists());
}
