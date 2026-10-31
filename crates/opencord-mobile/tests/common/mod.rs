//! A real server for the facade's tests, as `opencord-core`'s tests start
//! one.

#![allow(dead_code)]

use opencord_common::address::format_fingerprint;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};

pub struct TestServer {
    pub handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        config.voice.udp_port = 0;
        config.music.audio_udp_port = 0;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }

    pub fn fingerprint(&self) -> String {
        format_fingerprint(&self.handle.fingerprint)
    }

    pub fn claim_token(&self) -> Option<String> {
        self.handle.claim_token.clone()
    }
}

/// A small PNG, big enough for the server to make a thumbnail of.
pub fn png() -> Vec<u8> {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64, 64, image::Rgb([1, 2, 3])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}
