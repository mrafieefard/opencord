//! Non-secret local state: saved servers, pinned certificate fingerprints
//! and settings, kept as JSON in the app's data directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use opencord_common::address::{Fingerprint, format_fingerprint, parse_fingerprint};
use serde::{Deserialize, Serialize};

pub const STORE_FILE: &str = "opencord.json";
const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedServer {
    /// `host:port`, which identifies the server locally.
    pub key: String,
    pub host: String,
    pub port: u16,
    pub name: String,
    /// The user's id on that server, once known.
    pub user_id: Option<i64>,
    pub added_at_ms: i64,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct StoreData {
    version: u32,
    servers: Vec<SavedServer>,
    /// Server key -> lowercase hex SHA-256 of its certificate.
    pins: BTreeMap<String, String>,
    settings: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not read or write {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is damaged: {source}")]
    Corrupt {
        path: PathBuf,
        source: serde_json::Error,
    },
}

#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    data: StoreData,
}

impl Store {
    /// Opens the store in `dir`, empty if it does not exist yet.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        let path = dir.join(STORE_FILE);
        let data = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| StoreError::Corrupt {
                path: path.clone(),
                source,
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoreData {
                version: CURRENT_VERSION,
                ..StoreData::default()
            },
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        Ok(Self { path, data })
    }

    /// In the order they were added.
    pub fn servers(&self) -> &[SavedServer] {
        &self.data.servers
    }

    pub fn server(&self, key: &str) -> Option<&SavedServer> {
        self.data.servers.iter().find(|server| server.key == key)
    }

    /// Adds the server, or updates it in place if the key is known.
    pub fn upsert_server(&mut self, server: SavedServer) -> Result<(), StoreError> {
        match self
            .data
            .servers
            .iter_mut()
            .find(|known| known.key == server.key)
        {
            Some(known) => *known = server,
            None => self.data.servers.push(server),
        }
        self.save()
    }

    /// Returns whether the server was known.
    pub fn remove_server(&mut self, key: &str) -> Result<bool, StoreError> {
        let before = self.data.servers.len();
        self.data.servers.retain(|server| server.key != key);
        let removed = self.data.servers.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn pin(&self, key: &str) -> Option<Fingerprint> {
        self.data
            .pins
            .get(key)
            .and_then(|hex| parse_fingerprint(hex).ok())
    }

    pub fn set_pin(&mut self, key: &str, fingerprint: &Fingerprint) -> Result<(), StoreError> {
        self.data
            .pins
            .insert(key.to_owned(), format_fingerprint(fingerprint));
        self.save()
    }

    pub fn remove_pin(&mut self, key: &str) -> Result<(), StoreError> {
        if self.data.pins.remove(key).is_some() {
            self.save()?;
        }
        Ok(())
    }

    pub fn pins(&self) -> Vec<(String, Fingerprint)> {
        self.data
            .pins
            .iter()
            .filter_map(|(key, hex)| Some((key.clone(), parse_fingerprint(hex).ok()?)))
            .collect()
    }

    pub fn setting(&self, key: &str) -> Option<&str> {
        self.data.settings.get(key).map(String::as_str)
    }

    /// `None` removes the setting.
    pub fn set_setting(&mut self, key: &str, value: Option<String>) -> Result<(), StoreError> {
        match value {
            Some(value) => self.data.settings.insert(key.to_owned(), value),
            None => self.data.settings.remove(key),
        };
        self.save()
    }

    /// Writes to a temporary file first so a crash never leaves a half
    /// written store.
    fn save(&self) -> Result<(), StoreError> {
        let io_error = |source| StoreError::Io {
            path: self.path.clone(),
            source,
        };
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(io_error)?;
        }
        let text =
            serde_json::to_string_pretty(&self.data).map_err(|source| StoreError::Corrupt {
                path: self.path.clone(),
                source,
            })?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, text).map_err(io_error)?;
        fs::rename(&temporary, &self.path).map_err(io_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(key: &str, name: &str) -> SavedServer {
        let (host, port) = key.split_once(':').unwrap();
        SavedServer {
            key: key.to_owned(),
            host: host.to_owned(),
            port: port.parse().unwrap(),
            name: name.to_owned(),
            user_id: None,
            added_at_ms: 0,
        }
    }

    #[test]
    fn starts_empty() {
        let dir = tempfile::tempdir().unwrap();

        let store = Store::open(dir.path()).unwrap();

        assert!(store.servers().is_empty());
        assert!(store.pins().is_empty());
    }

    #[test]
    fn servers_persist_in_order_and_update_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.upsert_server(server("b.example:7710", "B")).unwrap();
        store.upsert_server(server("a.example:7710", "A")).unwrap();
        store
            .upsert_server(server("b.example:7710", "B renamed"))
            .unwrap();

        let reopened = Store::open(dir.path()).unwrap();

        let names: Vec<&str> = reopened.servers().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["B renamed", "A"]);
        assert_eq!(reopened.server("a.example:7710").unwrap().name, "A");
    }

    #[test]
    fn removing_a_server() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.upsert_server(server("a.example:7710", "A")).unwrap();

        assert!(store.remove_server("a.example:7710").unwrap());
        assert!(!store.remove_server("a.example:7710").unwrap());
        assert!(Store::open(dir.path()).unwrap().servers().is_empty());
    }

    #[test]
    fn pins_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.set_pin("a.example:7710", &[0xab; 32]).unwrap();

        let mut reopened = Store::open(dir.path()).unwrap();

        assert_eq!(reopened.pin("a.example:7710"), Some([0xab; 32]));
        assert_eq!(
            reopened.pins(),
            vec![("a.example:7710".to_owned(), [0xab; 32])]
        );
        reopened.remove_pin("a.example:7710").unwrap();
        assert_eq!(Store::open(dir.path()).unwrap().pin("a.example:7710"), None);
    }

    #[test]
    fn settings_can_be_set_and_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.set_setting("theme", Some("dark".to_owned())).unwrap();

        assert_eq!(
            Store::open(dir.path()).unwrap().setting("theme"),
            Some("dark")
        );
        store.set_setting("theme", None).unwrap();
        assert_eq!(Store::open(dir.path()).unwrap().setting("theme"), None);
    }

    #[test]
    fn a_damaged_file_is_reported_not_discarded() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(STORE_FILE), "{not json").unwrap();

        assert!(matches!(
            Store::open(dir.path()),
            Err(StoreError::Corrupt { .. })
        ));
        assert_eq!(
            fs::read_to_string(dir.path().join(STORE_FILE)).unwrap(),
            "{not json"
        );
    }

    #[test]
    fn leaves_no_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();

        store.upsert_server(server("a.example:7710", "A")).unwrap();

        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(files.len(), 1);
    }
}
