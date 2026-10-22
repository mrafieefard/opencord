//! Profile pictures and server icons on this device. This person's own
//! picture is kept beside their identity in the data directory and sent
//! to every server, as the display name is. Others' pictures and server
//! icons are fetched by their hash and kept once for every server, at most
//! 50 MB, the least recently shown going first.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use opencord_common::avatar;
use opencord_proto::v1 as proto;
use prost::Message as _;

use crate::api::types::{CoreError, ErrorCode, ServerInfo, User};
use crate::artwork::{store, touch};
use crate::client::Client;
use crate::convert;
use crate::http::{HttpResponse, MediaHttp};

/// This person's picture, in the data directory.
const OWN: &str = "avatar.png";
/// Others' pictures, inside the data directory.
const DIR: &str = "avatars";
const CACHE_BYTES: u64 = 50 * 1024 * 1024;
/// A stored picture is a 256-pixel PNG, far under this.
const RESPONSE_BYTES: usize = 2 * 1024 * 1024;

pub(crate) struct Avatars {
    data_dir: PathBuf,
    media: MediaHttp,
    /// One fetch per picture at a time; others wait for it.
    fetching: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

fn storage(error: impl std::fmt::Display) -> CoreError {
    CoreError::Storage {
        message: error.to_string(),
    }
}

fn invalid(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidInput {
        message: error.to_string(),
    }
}

/// A picture made from the file at `source`.
fn picture_from(source: &str) -> Result<Vec<u8>, CoreError> {
    let metadata = fs::metadata(source).map_err(invalid)?;
    if metadata.len() > avatar::MAX_UPLOAD_BYTES as u64 {
        return Err(invalid(avatar::AvatarError::TooLarge));
    }
    let bytes = fs::read(source).map_err(invalid)?;
    avatar::normalize(&bytes).map_err(invalid)
}

/// The result an endpoint answered with, or its error.
fn answer<T>(
    response: HttpResponse,
    pick: impl FnOnce(proto::response::Result) -> Option<T>,
) -> Result<T, CoreError> {
    let decoded = proto::Response::decode(response.body.as_slice())
        .ok()
        .and_then(|response| response.result);
    match decoded {
        Some(proto::response::Result::Error(error)) => Err(convert::error(error)),
        Some(result) => pick(result).ok_or_else(|| invalid("an unexpected answer")),
        None => Err(invalid(format!("the server answered {}", response.status))),
    }
}

impl Avatars {
    pub fn new(data_dir: &Path, media: MediaHttp) -> Self {
        Self {
            data_dir: data_dir.to_owned(),
            media,
            fetching: Mutex::default(),
        }
    }

    fn own_path(&self) -> PathBuf {
        self.data_dir.join(OWN)
    }

    /// Makes this person's picture from the file at `source`, or removes
    /// it; its hash.
    pub fn set_own(&self, source: Option<&str>) -> Result<Option<String>, CoreError> {
        let path = self.own_path();
        let Some(source) = source else {
            match fs::remove_file(&path) {
                Ok(()) => return Ok(None),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(storage(error)),
            }
        };
        let png = picture_from(source)?;
        let partial = path.with_extension("png.partial");
        fs::write(&partial, &png).map_err(storage)?;
        fs::rename(&partial, &path).map_err(storage)?;
        Ok(Some(avatar::hash(&png)))
    }

    /// This person's picture's file, if they have one.
    pub fn own_file(&self) -> Option<String> {
        let path = self.own_path();
        path.is_file().then(|| path.to_string_lossy().into_owned())
    }

    /// This person's picture's hash, if they have one.
    pub fn own_hash(&self) -> Option<String> {
        fs::read(self.own_path()).ok().map(|png| avatar::hash(&png))
    }

    /// Sends this person's picture, or that there is none, to the server
    /// at `key`; the user the server now shows.
    pub async fn upload_own(&self, client: &Client, key: &str) -> Result<User, CoreError> {
        let response = match fs::read(self.own_path()) {
            Ok(png) => {
                self.media
                    .request(client, key, "POST", "/media/avatar", &png, RESPONSE_BYTES)
                    .await?
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.media
                    .request(client, key, "DELETE", "/media/avatar", &[], RESPONSE_BYTES)
                    .await?
            }
            Err(error) => return Err(storage(error)),
        };
        answer(response, |result| match result {
            proto::response::Result::User(user) => Some(convert::user(user)),
            _ => None,
        })
    }

    /// The server's icon, made from the file at `source`, or none.
    pub async fn set_icon(
        &self,
        client: &Client,
        key: &str,
        source: Option<String>,
    ) -> Result<ServerInfo, CoreError> {
        let response = match source {
            Some(source) => {
                let png = tokio::task::spawn_blocking(move || picture_from(&source))
                    .await
                    .map_err(storage)??;
                self.media
                    .request(
                        client,
                        key,
                        "POST",
                        "/media/server-icon",
                        &png,
                        RESPONSE_BYTES,
                    )
                    .await?
            }
            None => {
                self.media
                    .request(
                        client,
                        key,
                        "DELETE",
                        "/media/server-icon",
                        &[],
                        RESPONSE_BYTES,
                    )
                    .await?
            }
        };
        answer(response, |result| match result {
            proto::response::Result::Server(info) => Some(convert::server_info(info)),
            _ => None,
        })
    }

    /// The path of the picture named `hash`, fetched from the server at
    /// `key` unless it is here already.
    pub async fn get(&self, client: &Client, key: &str, hash: &str) -> Result<String, CoreError> {
        if !avatar::valid_hash(hash) {
            return Err(invalid(format!("{hash} is not a picture's name")));
        }
        let dir = self.data_dir.join(DIR);
        let path = dir.join(format!("{hash}.png"));
        if touch(&path) {
            return Ok(path.to_string_lossy().into_owned());
        }
        let turn = Arc::clone(
            self.fetching
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(hash.to_owned())
                .or_default(),
        );
        let _turn = turn.lock().await;
        let fetched = if touch(&path) {
            Ok(())
        } else {
            self.fetch(client, key, hash, &dir, &path).await
        };
        self.fetching
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(hash);
        fetched.map(|()| path.to_string_lossy().into_owned())
    }

    async fn fetch(
        &self,
        client: &Client,
        key: &str,
        hash: &str,
        dir: &Path,
        path: &Path,
    ) -> Result<(), CoreError> {
        let response = self
            .media
            .request(
                client,
                key,
                "GET",
                &format!("/media/avatars/{hash}"),
                &[],
                RESPONSE_BYTES,
            )
            .await?;
        // Kept only when it is the picture asked for.
        if response.status != 200 || avatar::hash(&response.body) != hash {
            return Err(CoreError::Server {
                code: ErrorCode::NotFound,
                message: "the picture is not available".to_owned(),
                retry_after_ms: None,
            });
        }
        let (dir, path) = (dir.to_owned(), path.to_owned());
        tokio::task::spawn_blocking(move || store(&dir, &path, &response.body, CACHE_BYTES))
            .await
            .map_err(storage)?
            .map_err(storage)
    }
}
