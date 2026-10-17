//! Pebble, Let's Encrypt's test CA, for certificate tests; and a TLS
//! client to see which certificate the server serves.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rustls::pki_types::{CertificateDer, ServerName};
use serde_json::json;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use super::TestServer;

const PEBBLE: &str = "ghcr.io/letsencrypt/pebble";
const CHALLTESTSRV: &str = "ghcr.io/letsencrypt/pebble-challtestsrv";

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Pebble's and its name server's programs, copied out of their Docker
/// images once: run here, they share this machine's loopback, which
/// containers do not everywhere.
pub fn pebble_programs() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pebble");
        if !dir.join("pebble").exists() {
            std::fs::create_dir_all(&dir).unwrap();
            let copy = |image: &str, from: &str, to: &str| {
                let created = docker(&["create", image]);
                docker(&[
                    "cp",
                    &format!("{created}:{from}"),
                    &dir.join(to).display().to_string(),
                ]);
                docker(&["rm", &created]);
            };
            copy(CHALLTESTSRV, "/app", "challtestsrv");
            copy(PEBBLE, "/test/certs/localhost", "localhost");
            copy(PEBBLE, "/app", "pebble");
        }
        dir
    })
}

pub fn docker(args: &[&str]) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("Docker is installed");
    assert!(
        output.status.success(),
        "docker {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Pebble and its name server, stopped when dropped.
pub struct Pebble {
    processes: Vec<Child>,
    pub directory: String,
    management: u16,
    pub dns_management: u16,
    dir: tempfile::TempDir,
}

impl Pebble {
    /// Pebble checking HTTP-01 on `http_port` and TLS-ALPN-01 on
    /// `tls_port`.
    pub async fn start(http_port: u16, tls_port: u16) -> Self {
        // A port picked free may be taken before Pebble binds it.
        let mut failed = String::new();
        for _ in 0..3 {
            let pebble = Self::try_start(http_port, tls_port);
            match pebble.wait_until_up().await {
                Ok(()) => return pebble,
                Err(why) => failed = why,
            }
        }
        panic!("Pebble did not start: {failed}");
    }

    fn try_start(http_port: u16, tls_port: u16) -> Self {
        let programs = pebble_programs();
        let (listen, management, dns, dns_management) =
            (free_port(), free_port(), free_port(), free_port());
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("pebble.json");
        std::fs::write(
            &config,
            json!({ "pebble": {
                "listenAddress": format!("127.0.0.1:{listen}"),
                "managementListenAddress": format!("127.0.0.1:{management}"),
                "certificate": programs.join("localhost/cert.pem"),
                "privateKey": programs.join("localhost/key.pem"),
                "httpPort": http_port,
                "tlsPort": tls_port,
                "ocspResponderURL": "",
                "externalAccountBindingRequired": false,
                "retryAfter": { "authz": 1, "order": 1 },
                "keyAlgorithm": "ecdsa",
                "profiles": { "default": { "description": "", "validityPeriod": 7_776_000 } }
            }})
            .to_string(),
        )
        .unwrap();
        let mut pebble = Self {
            processes: Vec::new(),
            directory: format!("https://localhost:{listen}/dir"),
            management,
            dns_management,
            dir,
        };
        pebble.run(
            "challtestsrv",
            &[
                "-defaultIPv4",
                "127.0.0.1",
                "-defaultIPv6",
                "",
                "-dnsserver",
                &format!("127.0.0.1:{dns}"),
                "-management",
                &format!("127.0.0.1:{dns_management}"),
                "-http01",
                "",
                "-https01",
                "",
                "-tlsalpn01",
                "",
                "-doh",
                "",
            ],
        );
        pebble.run(
            "pebble",
            &[
                "-config",
                &config.display().to_string(),
                "-dnsserver",
                &format!("127.0.0.1:{dns}"),
            ],
        );
        pebble
    }

    fn run(&mut self, program: &str, args: &[&str]) {
        let log = std::fs::File::create(self.dir.path().join(format!("{program}.log"))).unwrap();
        let child = Command::new(pebble_programs().join(program))
            .args(args)
            .env("PEBBLE_VA_NOSLEEP", "1")
            .env("PEBBLE_WFE_NONCEREJECT", "0")
            .env("PEBBLE_AUTHZREUSE", "0")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        self.processes.push(child);
    }

    fn client() -> reqwest::Client {
        let root = std::fs::read_to_string(fixture("pebble.minica.pem")).unwrap();
        reqwest::Client::builder()
            .use_preconfigured_tls(trusting(first_der(&root)))
            .build()
            .unwrap()
    }

    async fn wait_until_up(&self) -> Result<(), String> {
        let client = Self::client();
        let mut last = None;
        for _ in 0..100 {
            match client.get(&self.directory).send().await {
                Ok(_) => return Ok(()),
                Err(error) => last = Some(format!("{error:?}")),
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(format!("{last:?}\n{}", self.logs()))
    }

    fn logs(&self) -> String {
        ["challtestsrv", "pebble"]
            .iter()
            .map(|program| {
                std::fs::read_to_string(self.dir.path().join(format!("{program}.log")))
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The root Pebble issues under; it makes a new one each start.
    pub async fn root(&self) -> CertificateDer<'static> {
        let pem = Self::client()
            .get(format!("https://localhost:{}/roots/0", self.management))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        first_der(&pem)
    }
}

impl Drop for Pebble {
    fn drop(&mut self) {
        for process in &mut self.processes {
            let _ = process.kill();
            let _ = process.wait();
        }
        if std::thread::panicking() {
            eprintln!("{}", self.logs());
        }
    }
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The first certificate in a PEM text.
pub fn first_der(pem: &str) -> CertificateDer<'static> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(pem.as_bytes()).unwrap();
    CertificateDer::from(pem.contents)
}

/// A TLS client that trusts only `root`.
pub fn trusting(root: CertificateDer<'static>) -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(root).unwrap();
    rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

/// The certificate the server serves for `host`, checked against `root`.
pub async fn served(server: &TestServer, host: &str, root: CertificateDer<'static>) -> Vec<u8> {
    let config = trusting(root);
    let tcp = TcpStream::connect(server.handle.local_addr).await.unwrap();
    let tls = TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from(host.to_owned()).unwrap(), tcp)
        .await
        .expect("the served certificate checks out for the name");
    tls.get_ref().1.peer_certificates().unwrap()[0].to_vec()
}
