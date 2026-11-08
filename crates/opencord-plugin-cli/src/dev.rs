use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use opencord_common::address::{Fingerprint, parse_fingerprint};
use opencord_plugins::manifest;
use opencord_plugins::package::Package;
use opencord_proto::v1 as proto;
use prost::Message as _;
use proto::response::Result as Reply;
use rustls::DigitallySignedStruct;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};

pub struct DevOptions<'a> {
    pub server: &'a str,
    pub fingerprint: Option<&'a str>,
    pub token: &'a str,
    pub folder: &'a Path,
}

#[derive(Debug)]
struct PinnedOrWebpki {
    pinned_fingerprint: Option<Fingerprint>,
    webpki: Arc<rustls::client::WebPkiServerVerifier>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedOrWebpki {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if let Some(expected) = self.pinned_fingerprint {
            let actual: Fingerprint = Sha256::digest(end_entity.as_ref()).into();
            if actual == expected {
                return Ok(ServerCertVerified::assertion());
            }
        }
        self.webpki
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.webpki.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.webpki.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

pub fn create_http_client(fingerprint_str: Option<&str>) -> Result<reqwest::Client, String> {
    let pinned_fingerprint = match fingerprint_str {
        Some(fp) => Some(parse_fingerprint(fp).map_err(|e| format!("invalid fingerprint: {e}"))?),
        None => None,
    };

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let algorithms = provider.signature_verification_algorithms;
    let root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let webpki = rustls::client::WebPkiServerVerifier::builder_with_provider(
        Arc::new(root_store),
        provider.clone(),
    )
    .build()
    .map_err(|e| format!("failed to build webpki verifier: {e}"))?;

    let verifier = Arc::new(PinnedOrWebpki {
        pinned_fingerprint,
        webpki,
        algorithms,
    });

    let client_config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("rustls config error: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    reqwest::Client::builder()
        .tls_backend_preconfigured(client_config)
        .build()
        .map_err(|e| format!("failed to build reqwest client: {e}"))
}

pub fn utc_time(at_ms: i64) -> String {
    let days = at_ms.div_euclid(86_400_000);
    let ms = at_ms.rem_euclid(86_400_000);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1024 + doe / 1460 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let secs = ms / 1000;
    let millis = ms % 1000;
    let h = secs / 3600;
    let min = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{millis:03}Z")
}

pub fn format_log_line(line: &proto::PluginLogLine) -> String {
    let level = match proto::PluginLogLevel::try_from(line.level) {
        Ok(proto::PluginLogLevel::Warn) => "WARN",
        Ok(proto::PluginLogLevel::Error) => "ERROR",
        Ok(proto::PluginLogLevel::Action) => "ACTION",
        _ => "INFO",
    };
    format!("{} {level} {}", utc_time(line.at_ms), line.text)
}

fn load_manifest_id(folder: &Path) -> Result<String, String> {
    let manifest_path = folder.join("manifest.lua");
    let content = fs::read_to_string(&manifest_path).map_err(|e| {
        format!(
            "failed to read manifest at '{}': {e}",
            manifest_path.display()
        )
    })?;
    let manifest =
        manifest::evaluate(&content).map_err(|e| format!("failed to evaluate manifest: {e}"))?;
    Ok(manifest.id)
}

pub async fn push_once(options: &DevOptions<'_>) -> Result<String, String> {
    let client = create_http_client(options.fingerprint)?;
    let id = load_manifest_id(options.folder)?;

    let package = Package::from_dir(options.folder)
        .map_err(|e| format!("failed to read plugin folder: {e}"))?;
    let zip_bytes = package.write_zip();

    let url = format!("https://{}/plugins/dev/{id}", options.server);
    let resp = client
        .put(&url)
        .header("Authorization", format!("Bearer {}", options.token))
        .header("Content-Type", "application/zip")
        .body(zip_bytes)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    let status = resp.status();
    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("failed to read response: {e}"))?;

    let response = proto::Response::decode(&*body)
        .map_err(|e| format!("invalid response proto (status {status}): {e}"))?;

    match (status.as_u16(), response.result) {
        (200, Some(Reply::Plugin(plugin))) => {
            let msg = format!("Installed {}, running", plugin.version);
            println!("{msg}");
            Ok(msg)
        }
        (202, Some(Reply::PluginReview(_))) => {
            let msg = "Waiting for an admin to review it on the Plugins page".to_string();
            println!("{msg}");
            Ok(msg)
        }
        (_, Some(Reply::Error(err))) => {
            let msg = format!("server error: {}", err.message);
            eprintln!("{msg}");
            Err(msg)
        }
        (status_code, _) => {
            let msg = format!("unexpected server status {status_code}");
            eprintln!("{msg}");
            Err(msg)
        }
    }
}

pub async fn poll_logs_once(options: &DevOptions<'_>, after: u64) -> Result<Vec<String>, String> {
    let client = create_http_client(options.fingerprint)?;
    let id = load_manifest_id(options.folder)?;

    let url = format!(
        "https://{}/plugins/dev/{id}/log?after={after}",
        options.server
    );
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", options.token))
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    let status = resp.status();
    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("failed to read response body: {e}"))?;

    let response = proto::Response::decode(&*body)
        .map_err(|e| format!("invalid response proto (status {status}): {e}"))?;

    match response.result {
        Some(Reply::PluginLog(log)) => {
            let lines = log.lines.iter().map(format_log_line).collect();
            Ok(lines)
        }
        Some(Reply::Error(err)) => Err(format!("error reading logs: {}", err.message)),
        _ => Err(format!("unexpected response reading logs: status {status}")),
    }
}

fn folder_snapshot(folder: &Path) -> BTreeMap<PathBuf, (SystemTime, u64)> {
    let mut map = BTreeMap::new();
    let walker = match fs::read_dir(folder) {
        Ok(read_dir) => read_dir,
        Err(_) => return map,
    };

    let mut stack = Vec::new();
    for entry in walker.flatten() {
        stack.push(entry.path());
    }

    while let Some(path) = stack.pop() {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name.starts_with('.') || name.ends_with(".zip") || name == "signing.key" {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if meta.is_dir() {
                if let Ok(entries) = fs::read_dir(&path) {
                    for entry in entries.flatten() {
                        stack.push(entry.path());
                    }
                }
            } else if meta.is_file()
                && let Ok(rel) = path.strip_prefix(folder)
            {
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                map.insert(rel.to_path_buf(), (mtime, meta.len()));
            }
        }
    }
    map
}

pub async fn run_dev(options: DevOptions<'_>) -> Result<(), String> {
    let client = create_http_client(options.fingerprint)?;
    let id = load_manifest_id(options.folder)?;

    println!(
        "Watching '{}' for changes (dev mode for plugin '{id}')...",
        options.folder.display()
    );

    // Initial push with retry on connection errors
    loop {
        match push_once(&options).await {
            Ok(_) => break,
            Err(e) => {
                eprintln!("{e}. Retrying in 2 seconds...");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    // Spawn long-polling log streamer
    let server_str = options.server.to_string();
    let token_str = options.token.to_string();
    let id_str = id.clone();
    let client_for_logs = client.clone();
    tokio::spawn(async move {
        let mut cursor = 0u64;
        loop {
            let url = format!("https://{server_str}/plugins/dev/{id_str}/log?after={cursor}");
            match client_for_logs
                .get(&url)
                .header("Authorization", format!("Bearer {token_str}"))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if let Ok(body) = resp.bytes().await
                        && let Ok(response) = proto::Response::decode(&*body)
                        && let Some(Reply::PluginLog(log)) = response.result
                    {
                        for line in log.lines {
                            if line.seq > cursor {
                                cursor = line.seq;
                            }
                            println!("{}", format_log_line(&line));
                        }
                    } else if !status.is_success() {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
                Err(_) => {
                    // Connection error / server restart: retry in 2 seconds
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    });

    // File watching loop (poll every 300ms)
    let mut last_snapshot = folder_snapshot(options.folder);
    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let current_snapshot = folder_snapshot(options.folder);
        if current_snapshot != last_snapshot {
            last_snapshot = current_snapshot;
            println!("Changes detected, packing and pushing...");
            loop {
                match push_once(&options).await {
                    Ok(_) => break,
                    Err(e) => {
                        eprintln!("{e}. Retrying in 2 seconds...");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
    }
}
