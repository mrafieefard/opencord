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
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
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

#[derive(Debug)]
pub enum PushError {
    Network(String),
    Refused(String),
}

impl std::fmt::Display for PushError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(s) => write!(f, "network error: {s}"),
            Self::Refused(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug)]
pub enum PollLogError {
    Network(String),
    Server(String),
}

impl std::fmt::Display for PollLogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(s) => write!(f, "network error: {s}"),
            Self::Server(s) => write!(f, "{s}"),
        }
    }
}

pub async fn push_once(options: &DevOptions<'_>) -> Result<String, PushError> {
    let client = create_http_client(options.fingerprint).map_err(PushError::Refused)?;
    let id = load_manifest_id(options.folder).map_err(PushError::Refused)?;

    let package = Package::from_dir(options.folder)
        .map_err(|e| PushError::Refused(format!("failed to read plugin folder: {e}")))?;
    let zip_bytes = package.write_zip();

    let url = format!("https://{}/plugins/dev/{id}", options.server);
    let resp = client
        .put(&url)
        .header("Authorization", format!("Bearer {}", options.token))
        .header("Content-Type", "application/zip")
        .body(zip_bytes)
        .send()
        .await
        .map_err(|e| PushError::Network(e.to_string()))?;

    let status = resp.status();
    let body = resp
        .bytes()
        .await
        .map_err(|e| PushError::Network(format!("failed to read response: {e}")))?;

    let response = proto::Response::decode(&*body).map_err(|e| {
        PushError::Refused(format!("invalid response proto (status {status}): {e}"))
    })?;

    match (status.as_u16(), response.result) {
        (200, Some(Reply::Plugin(plugin))) => {
            let msg = format!("Installed {}, running", plugin.version);
            Ok(msg)
        }
        (202, Some(Reply::PluginReview(_))) => {
            let msg = "Waiting for an admin to review it on the Plugins page".to_string();
            Ok(msg)
        }
        (_, Some(Reply::Error(err))) => {
            Err(PushError::Refused(format!("server error: {}", err.message)))
        }
        (status_code, _) => Err(PushError::Refused(format!(
            "unexpected server status {status_code}"
        ))),
    }
}

pub async fn poll_logs_once(
    options: &DevOptions<'_>,
    after: u64,
) -> Result<Vec<proto::PluginLogLine>, PollLogError> {
    let client = create_http_client(options.fingerprint).map_err(PollLogError::Server)?;
    let id = load_manifest_id(options.folder).map_err(PollLogError::Server)?;

    let url = format!(
        "https://{}/plugins/dev/{id}/log?after={after}",
        options.server
    );
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", options.token))
        .send()
        .await
        .map_err(|e| PollLogError::Network(e.to_string()))?;

    let status = resp.status();
    let body = resp
        .bytes()
        .await
        .map_err(|e| PollLogError::Network(format!("failed to read response body: {e}")))?;

    let response = proto::Response::decode(&*body).map_err(|e| {
        PollLogError::Server(format!("invalid response proto (status {status}): {e}"))
    })?;

    match response.result {
        Some(Reply::PluginLog(log)) => Ok(log.lines),
        Some(Reply::Error(err)) => Err(PollLogError::Server(format!(
            "error reading logs: {}",
            err.message
        ))),
        _ => Err(PollLogError::Server(format!(
            "unexpected response reading logs: status {status}"
        ))),
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
        let Ok(rel) = path.strip_prefix(folder) else {
            continue;
        };
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if rel.components().count() == 1 && opencord_plugins::package::is_ignored_root_file(name) {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if meta.is_dir() {
                if let Ok(entries) = fs::read_dir(&path) {
                    for entry in entries.flatten() {
                        stack.push(entry.path());
                    }
                }
            } else if meta.is_file() {
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                map.insert(rel.to_path_buf(), (mtime, meta.len()));
            }
        }
    }
    map
}

pub async fn run_dev(options: DevOptions<'_>) -> Result<(), String> {
    run_dev_with(options, std::io::stdout(), None).await
}

pub async fn run_dev_with<W: std::io::Write + Send + 'static>(
    options: DevOptions<'_>,
    writer: W,
    stop_rx: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<(), String> {
    let writer = Arc::new(std::sync::Mutex::new(writer));
    let id = load_manifest_id(options.folder)?;

    {
        let mut w = writer.lock().unwrap();
        let _ = writeln!(
            w,
            "Watching '{}' for changes (dev mode for plugin '{id}')...",
            options.folder.display()
        );
    }

    // Initial push with retry on connection errors
    loop {
        if let Some(ref rx) = stop_rx
            && *rx.borrow()
        {
            return Ok(());
        }
        match push_once(&options).await {
            Ok(msg) => {
                let mut w = writer.lock().unwrap();
                let _ = writeln!(w, "{msg}");
                break;
            }
            Err(PushError::Network(e)) => {
                {
                    let mut w = writer.lock().unwrap();
                    let _ = writeln!(w, "{e}. Retrying in 2 seconds...");
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(PushError::Refused(e)) => {
                let mut w = writer.lock().unwrap();
                let _ = writeln!(w, "{e}");
                break;
            }
        }
    }

    // Spawn long-polling log streamer
    let log_server = options.server.to_string();
    let log_fingerprint = options.fingerprint.map(|s| s.to_string());
    let log_token = options.token.to_string();
    let log_folder = options.folder.to_path_buf();
    let log_writer = Arc::clone(&writer);
    let log_stop = stop_rx.clone();
    let log_handle = tokio::spawn(async move {
        let log_options = DevOptions {
            server: &log_server,
            fingerprint: log_fingerprint.as_deref(),
            token: &log_token,
            folder: &log_folder,
        };
        let mut cursor = 0u64;
        let mut seen = std::collections::HashSet::new();
        loop {
            if let Some(ref rx) = log_stop
                && *rx.borrow()
            {
                break;
            }
            match poll_logs_once(&log_options, cursor).await {
                Ok(lines) => {
                    for line in lines {
                        if line.seq > cursor {
                            cursor = line.seq;
                        }
                        let key = (line.at_ms, line.level, line.text.clone());
                        if seen.insert(key) {
                            if seen.len() > 2048 {
                                seen.clear();
                            }
                            let formatted = format_log_line(&line);
                            let mut w = log_writer.lock().unwrap();
                            let _ = writeln!(w, "{formatted}");
                        }
                    }
                }
                Err(PollLogError::Network(_)) => {
                    cursor = 0;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                Err(PollLogError::Server(e)) => {
                    {
                        let mut w = log_writer.lock().unwrap();
                        let _ = writeln!(w, "{e}");
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    });

    // File watching loop (poll every 300ms)
    let mut last_snapshot = folder_snapshot(options.folder);
    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if let Some(ref rx) = stop_rx
            && *rx.borrow()
        {
            break;
        }
        let current_snapshot = folder_snapshot(options.folder);
        if current_snapshot != last_snapshot {
            last_snapshot = current_snapshot;
            {
                let mut w = writer.lock().unwrap();
                let _ = writeln!(w, "Changes detected, packing and pushing...");
            }
            loop {
                if let Some(ref rx) = stop_rx
                    && *rx.borrow()
                {
                    break;
                }
                match push_once(&options).await {
                    Ok(msg) => {
                        let mut w = writer.lock().unwrap();
                        let _ = writeln!(w, "{msg}");
                        break;
                    }
                    Err(PushError::Network(e)) => {
                        {
                            let mut w = writer.lock().unwrap();
                            let _ = writeln!(w, "{e}. Retrying in 2 seconds...");
                        }
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                    Err(PushError::Refused(e)) => {
                        let mut w = writer.lock().unwrap();
                        let _ = writeln!(w, "{e}");
                        break;
                    }
                }
            }
        }
    }

    log_handle.abort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_time_formats_exact_civil_dates() {
        assert_eq!(utc_time(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(utc_time(1772236800000), "2026-02-28T00:00:00.000Z");
        assert_eq!(utc_time(1835395200000), "2028-02-29T00:00:00.000Z");
        assert_eq!(utc_time(1798761599999), "2026-12-31T23:59:59.999Z");
        assert_eq!(utc_time(-86400000), "1969-12-31T00:00:00.000Z");
    }
}
