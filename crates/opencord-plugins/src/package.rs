//! A plugin's package (spec §7.1): the files of its zip, checked.
//!
//! Every way in ([`Package::read_zip`], [`Package::from_dir`],
//! [`Package::insert`]) checks the same rules, so a `Package` always holds
//! files the server would accept. Sizes are counted while reading, never
//! taken from a zip's headers.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Cursor, Read, Write};
use std::path::Path;

use flate2::Crc;
use flate2::read::DeflateDecoder;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::limits::{
    PACKAGE_MAX_FILE_BYTES, PACKAGE_MAX_FILES, PACKAGE_MAX_RATIO, PACKAGE_MAX_UNPACKED_BYTES,
    PACKAGE_PATH_MAX_BYTES, PACKAGE_RATIO_MIN_BYTES,
};

/// The files a package may have at its root.
const ROOT_FILES: &[&str] = &["manifest.lua", "SIGNATURE", "KEY-ROTATION"];
const SCRIPTS: &[&str] = &["lua", "luau"];
const ASSETS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "ttf", "otf", "ogg", "mp3", "wav",
];

/// The type bits of a Unix mode, and a symbolic link's.
const FILE_TYPE: u32 = 0o170_000;
const SYMLINK: u32 = 0o120_000;

/// How many characters of a path an error shows.
const SHOWN_CHARS: usize = 256;

/// Why a package was refused. Each names the rule it broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    OutsideFolders(String),
    KindNotAllowed(String),
    BadPath(String),
    CaseClash {
        path: String,
        other: String,
    },
    Link(String),
    Encrypted(String),
    FileTooLarge(String),
    TooManyFiles,
    TooLarge,
    Ratio(String),
    NotZip,
    NoManifest,
    /// A file in a folder couldn't be read.
    Read {
        path: String,
        error: String,
    },
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageError::OutsideFolders(path) => write!(
                f,
                "{}: files can only be in server/, client/ or assets/",
                shown(path)
            ),
            PackageError::KindNotAllowed(path) => {
                write!(f, "{}: this kind of file isn't allowed there", shown(path))
            }
            PackageError::BadPath(path) => write!(f, "{}: the path isn't allowed", shown(path)),
            PackageError::CaseClash { path, other } => write!(
                f,
                "{} and {}: two files differ only by upper and lower case",
                shown(path),
                shown(other)
            ),
            PackageError::Link(path) => write!(f, "{}: links aren't allowed", shown(path)),
            PackageError::Encrypted(path) => {
                write!(f, "{}: encrypted files aren't allowed", shown(path))
            }
            PackageError::FileTooLarge(path) => write!(
                f,
                "{} is larger than {} MB",
                shown(path),
                PACKAGE_MAX_FILE_BYTES >> 20
            ),
            PackageError::TooManyFiles => {
                write!(f, "the plugin has more than {PACKAGE_MAX_FILES} files")
            }
            PackageError::TooLarge => write!(
                f,
                "the plugin is larger than {} MB unpacked",
                PACKAGE_MAX_UNPACKED_BYTES >> 20
            ),
            PackageError::Ratio(path) => {
                write!(f, "{} is compressed suspiciously well", shown(path))
            }
            PackageError::NotZip => f.write_str("this isn't a zip file"),
            PackageError::NoManifest => f.write_str("manifest.lua is missing"),
            PackageError::Read { path, error } => {
                write!(f, "{}: it can't be read: {error}", shown(path))
            }
        }
    }
}

impl std::error::Error for PackageError {}

/// A path as an error shows it: control characters replaced, and cut short,
/// since a refused path can be anything a zip holds.
pub(crate) fn shown(text: &str) -> String {
    let mut out: String = text
        .chars()
        .take(SHOWN_CHARS)
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect();
    if text.chars().nth(SHOWN_CHARS).is_some() {
        out.push('…');
    }
    out
}

/// Checks one path against the package's rules: its shape, then its folder,
/// then its kind.
pub(crate) fn check_path(path: &str) -> Result<(), PackageError> {
    let bad = || PackageError::BadPath(path.to_owned());
    if path.len() > PACKAGE_PATH_MAX_BYTES {
        return Err(bad());
    }
    let segments: Vec<&str> = path.split('/').collect();
    for segment in &segments {
        let allowed = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-');
        if segment.is_empty() || segment.starts_with('.') || !segment.bytes().all(allowed) {
            return Err(bad());
        }
    }
    let kinds = match segments.as_slice() {
        [name] if ROOT_FILES.contains(name) => return Ok(()),
        [_] => return Err(PackageError::OutsideFolders(path.to_owned())),
        ["server" | "client", ..] => SCRIPTS,
        ["assets", ..] => ASSETS,
        _ => return Err(PackageError::OutsideFolders(path.to_owned())),
    };
    let name = segments.last().copied().unwrap_or_default();
    let allowed = name.rsplit_once('.').is_some_and(|(_, extension)| {
        kinds
            .iter()
            .any(|kind| kind.eq_ignore_ascii_case(extension))
    });
    if !allowed {
        return Err(PackageError::KindNotAllowed(path.to_owned()));
    }
    Ok(())
}

/// Reads at most `limit + 1` bytes of `reader`, so a file over the limit is
/// seen without holding more of it.
pub(crate) fn unpack(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    bytes.shrink_to_fit();
    Ok(bytes)
}

/// A plugin's files, by path.
#[derive(Clone, PartialEq, Eq)]
pub struct Package {
    files: BTreeMap<String, Vec<u8>>,
}

impl fmt::Debug for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.files.iter().map(|(path, bytes)| (path, bytes.len())))
            .finish()
    }
}

impl Package {
    pub(crate) fn empty() -> Package {
        Package {
            files: BTreeMap::new(),
        }
    }

    /// Reads and checks a plugin's zip.
    pub fn read_zip(bytes: &[u8]) -> Result<Package, PackageError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| PackageError::NotZip)?;
        // The zip crate keeps one of two entries with the same name.
        if archive.has_duplicate_names() {
            return Err(PackageError::NotZip);
        }
        let mut package = Package::empty();
        for index in 0..archive.len() {
            // Raw: the zip crate's own reader stops at the header's size,
            // and sizes here are counted, not read from headers.
            let mut entry = archive
                .by_index_raw(index)
                .map_err(|_| PackageError::NotZip)?;
            if entry.is_dir() {
                continue;
            }
            let path = entry.name().map_err(|_| PackageError::NotZip)?.into_owned();
            package.check_new(&path)?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & FILE_TYPE == SYMLINK)
            {
                return Err(PackageError::Link(path));
            }
            if entry.encrypted() {
                return Err(PackageError::Encrypted(path));
            }
            let room = package.room();
            let crc = entry.crc32();
            let (bytes, packed) = match entry.compression() {
                CompressionMethod::Stored => {
                    let bytes = unpack(&mut entry, room).map_err(|_| PackageError::NotZip)?;
                    let packed = bytes.len() as u64;
                    (bytes, packed)
                }
                CompressionMethod::Deflated => {
                    let mut inflater = DeflateDecoder::new(&mut entry);
                    let bytes = unpack(&mut inflater, room).map_err(|_| PackageError::NotZip)?;
                    (bytes, inflater.total_in())
                }
                _ => return Err(PackageError::NotZip),
            };
            package.check_size(&path, bytes.len() as u64)?;
            let unpacked = bytes.len() as u64;
            if unpacked > PACKAGE_RATIO_MIN_BYTES
                && unpacked > packed.saturating_mul(PACKAGE_MAX_RATIO)
            {
                return Err(PackageError::Ratio(path));
            }
            let mut sum = Crc::new();
            sum.update(&bytes);
            if sum.sum() != crc {
                return Err(PackageError::NotZip);
            }
            package.files.insert(path, bytes);
        }
        package.check_manifest()?;
        Ok(package)
    }

    /// Reads and checks a plugin's folder, skipping files and folders whose
    /// names start with a dot.
    pub fn from_dir(dir: &Path) -> Result<Package, PackageError> {
        let mut package = Package::empty();
        package.add_dir(dir, "")?;
        package.check_manifest()?;
        Ok(package)
    }

    fn add_dir(&mut self, dir: &Path, prefix: &str) -> Result<(), PackageError> {
        let failed = |error: io::Error| PackageError::Read {
            path: if prefix.is_empty() { "." } else { prefix }.to_owned(),
            error: error.to_string(),
        };
        let mut entries = fs::read_dir(dir)
            .and_then(|entries| entries.collect::<io::Result<Vec<_>>>())
            .map_err(failed)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let path = if prefix.is_empty() {
                name.into_owned()
            } else {
                format!("{prefix}/{name}")
            };
            let kind = entry.file_type().map_err(|error| PackageError::Read {
                path: path.clone(),
                error: error.to_string(),
            })?;
            if kind.is_symlink() {
                return Err(PackageError::Link(path));
            }
            if kind.is_dir() {
                // Nothing under a path this long fits the rules.
                if path.len() > PACKAGE_PATH_MAX_BYTES {
                    return Err(PackageError::BadPath(path));
                }
                self.add_dir(&entry.path(), &path)?;
                continue;
            }
            self.check_new(&path)?;
            // A pipe or a device would block or never end.
            if !kind.is_file() {
                return Err(PackageError::KindNotAllowed(path));
            }
            let read = |error: io::Error| PackageError::Read {
                path: path.clone(),
                error: error.to_string(),
            };
            let file = fs::File::open(entry.path()).map_err(read)?;
            let bytes = unpack(file, self.room()).map_err(read)?;
            self.check_size(&path, bytes.len() as u64)?;
            self.files.insert(path, bytes);
        }
        Ok(())
    }

    /// Writes the package as a zip: deflated, in path order, with fixed
    /// dates, so the same files always make the same bytes.
    pub fn write_zip(&self) -> Vec<u8> {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(DateTime::default())
            .unix_permissions(0o644);
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let written = self
            .files
            .iter()
            .try_for_each(|(path, bytes)| {
                writer.start_file(path.as_str(), options)?;
                writer.write_all(bytes)?;
                Ok::<_, zip::result::ZipError>(())
            })
            .and_then(|()| writer.finish());
        // Writing to memory fails only past zip's limits (4 GB, 65,535
        // files), and a package's own limits are far below them.
        written
            .expect("a package fits in a zip without zip64")
            .into_inner()
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// Adds or replaces one file, under the same rules as reading a zip.
    pub fn insert(&mut self, path: &str, bytes: Vec<u8>) -> Result<(), PackageError> {
        let replaced = self.files.get(path).map(|old| old.len() as u64);
        if replaced.is_none() {
            self.check_new(path)?;
        }
        let size = bytes.len() as u64;
        if size > PACKAGE_MAX_FILE_BYTES {
            return Err(PackageError::FileTooLarge(path.to_owned()));
        }
        if self.total_bytes() - replaced.unwrap_or(0) + size > PACKAGE_MAX_UNPACKED_BYTES {
            return Err(PackageError::TooLarge);
        }
        self.files.insert(path.to_owned(), bytes);
        Ok(())
    }

    /// Unpacked size of all the files.
    pub fn total_bytes(&self) -> u64 {
        self.files.values().map(|bytes| bytes.len() as u64).sum()
    }

    /// What one more file may hold: the file limit, or what is left of the
    /// package's.
    pub(crate) fn room(&self) -> u64 {
        PACKAGE_MAX_FILE_BYTES.min(PACKAGE_MAX_UNPACKED_BYTES.saturating_sub(self.total_bytes()))
    }

    /// Checks a path not yet in the package: its rules, a case clash with
    /// one that is, and room for one more file.
    fn check_new(&self, path: &str) -> Result<(), PackageError> {
        check_path(path)?;
        if let Some(other) = self.paths().find(|other| other.eq_ignore_ascii_case(path)) {
            return Err(PackageError::CaseClash {
                path: other.to_owned(),
                other: path.to_owned(),
            });
        }
        if self.files.len() >= PACKAGE_MAX_FILES {
            return Err(PackageError::TooManyFiles);
        }
        Ok(())
    }

    /// Checks the size of a file read with [`Package::room`] as its limit.
    fn check_size(&self, path: &str, size: u64) -> Result<(), PackageError> {
        if size > PACKAGE_MAX_FILE_BYTES {
            return Err(PackageError::FileTooLarge(path.to_owned()));
        }
        if self.total_bytes() + size > PACKAGE_MAX_UNPACKED_BYTES {
            return Err(PackageError::TooLarge);
        }
        Ok(())
    }

    fn check_manifest(&self) -> Result<(), PackageError> {
        if self.files.contains_key("manifest.lua") {
            Ok(())
        } else {
            Err(PackageError::NoManifest)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{Cursor, Read, Write};

    use flate2::read::DeflateDecoder;
    use flate2::write::DeflateEncoder;
    use flate2::{Compression, Crc};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;
    use crate::limits::{PACKAGE_MAX_FILE_BYTES, PACKAGE_MAX_FILES};

    const MIB: usize = 1024 * 1024;
    const MANIFEST: &[u8] = b"return {}";

    /// A zip of `files`, deflated, as a zip tool makes one.
    pub(crate) fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (path, bytes) in files {
            writer.start_file(*path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    const STORED: u16 = 0;
    const DEFLATED: u16 = 8;
    const ENCRYPTED: u16 = 1;

    /// One entry of a zip written byte by byte, so its headers can lie.
    struct Raw<'a> {
        name: &'a str,
        method: u16,
        flags: u16,
        crc: u32,
        packed: &'a [u8],
        claimed_size: u32,
    }

    fn stored<'a>(name: &'a str, bytes: &'a [u8]) -> Raw<'a> {
        let mut crc = Crc::new();
        crc.update(bytes);
        Raw {
            name,
            method: STORED,
            flags: 0,
            crc: crc.sum(),
            packed: bytes,
            claimed_size: bytes.len() as u32,
        }
    }

    fn raw_zip(entries: &[Raw]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for entry in entries {
            let offset = out.len() as u32;
            let name = entry.name.as_bytes();
            let mut fields = Vec::new();
            fields.extend_from_slice(&entry.flags.to_le_bytes());
            fields.extend_from_slice(&entry.method.to_le_bytes());
            fields.extend_from_slice(&0u16.to_le_bytes()); // time
            fields.extend_from_slice(&0x21u16.to_le_bytes()); // 1980-01-01
            fields.extend_from_slice(&entry.crc.to_le_bytes());
            fields.extend_from_slice(&(entry.packed.len() as u32).to_le_bytes());
            fields.extend_from_slice(&entry.claimed_size.to_le_bytes());
            fields.extend_from_slice(&(name.len() as u16).to_le_bytes());
            fields.extend_from_slice(&0u16.to_le_bytes()); // extra field length

            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&fields);
            out.extend_from_slice(name);
            out.extend_from_slice(entry.packed);

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // made by
            central.extend_from_slice(&20u16.to_le_bytes()); // version needed
            central.extend_from_slice(&fields);
            central.extend_from_slice(&[0; 6]); // comment length, disk, internal attributes
            central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name);
        }
        let central_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]); // disk numbers
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment length
        out
    }

    /// A deflate stream of `mib` MiB of zeros, made in milliseconds: after
    /// the first MiB, each MiB's blocks only copy the zeros before them, so
    /// one MiB's blocks repeat.
    fn deflated_zeros(mib: usize) -> Vec<u8> {
        let zeros = vec![0u8; MIB];
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&zeros).unwrap();
        encoder.flush().unwrap();
        let first = encoder.get_ref().len();
        encoder.write_all(&zeros).unwrap();
        encoder.flush().unwrap();
        let second = encoder.get_ref().len();
        let all = encoder.finish().unwrap();

        let mut stream = all[..first].to_vec();
        for _ in 1..mib {
            stream.extend_from_slice(&all[first..second]);
        }
        stream.extend_from_slice(&all[second..]);
        stream
    }

    /// Counts the bytes read through it.
    struct Counting<R> {
        inner: R,
        count: u64,
    }

    impl<R: Read> Read for Counting<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(buf)?;
            self.count += read as u64;
            Ok(read)
        }
    }

    fn refused(zip: &[u8]) -> String {
        Package::read_zip(zip).unwrap_err().to_string()
    }

    #[test]
    fn deflated_zeros_are_zeros() {
        let mut unpacked = Vec::new();
        DeflateDecoder::new(&deflated_zeros(3)[..])
            .read_to_end(&mut unpacked)
            .unwrap();
        assert_eq!(unpacked.len(), 3 * MIB);
        assert!(unpacked.iter().all(|&b| b == 0));
    }

    // Catches: checking paths with `Path` (which reads `server\x.lua` as one
    // name on Unix), or only rejecting a leading `..`.
    #[test]
    fn refuses_each_bad_path() {
        let long = format!("server/{}.lua", "a".repeat(190));
        assert_eq!(long.len(), 201);
        let path = |p: &str| format!("{p}: the path isn't allowed");
        let outside = |p: &str| format!("{p}: files can only be in server/, client/ or assets/");
        let kind = |p: &str| format!("{p}: this kind of file isn't allowed there");
        let cases = [
            ("../x", path("../x")),
            ("/x", path("/x")),
            ("server\\x.lua", path("server\\x.lua")),
            ("server//x.lua", path("server//x.lua")),
            ("server/.x.lua", path("server/.x.lua")),
            ("server/../x.lua", path("server/../x.lua")),
            ("server/x y.lua", path("server/x y.lua")),
            ("server/é.lua", path("server/é.lua")),
            ("server/x.lua\0", path("server/x.lua\u{fffd}")),
            (long.as_str(), path(&long)),
            ("other/x.lua", outside("other/x.lua")),
            ("README.md", outside("README.md")),
            ("Server/x.lua", outside("Server/x.lua")),
            ("Manifest.lua", outside("Manifest.lua")),
            ("server/x.exe", kind("server/x.exe")),
            ("server/x", kind("server/x")),
            ("server/SIGNATURE", kind("server/SIGNATURE")),
            ("assets/x.lua", kind("assets/x.lua")),
            ("assets/x.svg", kind("assets/x.svg")),
            ("client/x.png", kind("client/x.png")),
        ];
        for (name, message) in cases {
            let zip = raw_zip(&[stored("manifest.lua", MANIFEST), stored(name, b"x")]);
            assert_eq!(refused(&zip), message, "{name:?}");
        }
        // insert keeps the same rules.
        let mut package = Package::read_zip(&zip_of(&[("manifest.lua", MANIFEST)])).unwrap();
        let error = package.insert("../x", b"x".to_vec()).unwrap_err();
        assert_eq!(error.to_string(), "../x: the path isn't allowed");
        assert!(package.get("../x").is_none());
    }

    #[test]
    fn accepts_every_allowed_kind() {
        let files: &[(&str, &[u8])] = &[
            ("manifest.lua", MANIFEST),
            ("SIGNATURE", b"s"),
            ("KEY-ROTATION", b"k"),
            ("server/main.luau", b"x"),
            ("server/lib/util.lua", b"x"),
            ("client/main.LUAU", b"x"),
            ("assets/icon.PNG", b"x"),
            ("assets/a.jpg", b"x"),
            ("assets/a.jpeg", b"x"),
            ("assets/a.webp", b"x"),
            ("assets/a.gif", b"x"),
            ("assets/fonts/A_b-c.1.ttf", b"x"),
            ("assets/a.otf", b"x"),
            ("assets/a.ogg", b"x"),
            ("assets/a.mp3", b"x"),
            ("assets/a.wav", b"x"),
        ];
        let package = Package::read_zip(&zip_of(files)).unwrap();
        assert_eq!(package.paths().count(), files.len());
    }

    // Catches: a case-sensitive duplicate check, which unpacks two files
    // into one on Windows and macOS.
    #[test]
    fn refuses_case_clashes() {
        let zip = zip_of(&[
            ("manifest.lua", MANIFEST),
            ("server/Main.lua", b"a"),
            ("server/main.lua", b"b"),
        ]);
        assert_eq!(
            refused(&zip),
            "server/Main.lua and server/main.lua: two files differ only by upper and lower case"
        );

        // Two entries with one name: one would silently replace the other.
        let zip = raw_zip(&[
            stored("manifest.lua", MANIFEST),
            stored("server/a.lua", b"a"),
            stored("server/a.lua", b"b"),
        ]);
        assert_eq!(refused(&zip), "this isn't a zip file");

        let mut package = Package::read_zip(&zip_of(&[("manifest.lua", MANIFEST)])).unwrap();
        package.insert("server/a.lua", b"a".to_vec()).unwrap();
        let error = package.insert("server/A.lua", b"b".to_vec()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "server/a.lua and server/A.lua: two files differ only by upper and lower case"
        );
        // The same path again replaces it.
        package.insert("server/a.lua", b"c".to_vec()).unwrap();
        assert_eq!(package.get("server/a.lua"), Some(&b"c"[..]));
    }

    // Catches: reading a link's target as the file's content, or skipping
    // links quietly.
    #[test]
    fn refuses_symlinks() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer.start_file("manifest.lua", options).unwrap();
        writer.write_all(MANIFEST).unwrap();
        writer
            .add_symlink("server/link.lua", "/etc/passwd", options)
            .unwrap();
        let zip = writer.finish().unwrap().into_inner();
        assert_eq!(refused(&zip), "server/link.lua: links aren't allowed");
    }

    #[test]
    fn refuses_encrypted_entries() {
        let entry = Raw {
            flags: ENCRYPTED,
            ..stored("server/x.lua", b"x")
        };
        let zip = raw_zip(&[stored("manifest.lua", MANIFEST), entry]);
        assert_eq!(
            refused(&zip),
            "server/x.lua: encrypted files aren't allowed"
        );
    }

    // Catches: unpacking an entry whole (300 MB of memory) before checking
    // its size.
    #[test]
    fn stops_a_zip_bomb_while_reading() {
        let stream = deflated_zeros(300);
        assert!(stream.len() < MIB, "{} bytes packed", stream.len());
        let bomb = Raw {
            name: "assets/bomb.wav",
            method: DEFLATED,
            flags: 0,
            crc: 0,
            packed: &stream,
            claimed_size: (300 * MIB) as u32,
        };
        let zip = raw_zip(&[stored("manifest.lua", MANIFEST), bomb]);
        assert_eq!(refused(&zip), "assets/bomb.wav is larger than 20 MB");

        // What read_zip pulls out of the inflater for that entry, counted.
        let room = Package::empty().room();
        let mut counted = Counting {
            inner: DeflateDecoder::new(&stream[..]),
            count: 0,
        };
        let held = unpack(&mut counted, room).unwrap();
        assert!(held.len() as u64 > PACKAGE_MAX_FILE_BYTES);
        assert!(counted.count <= 201 * MIB as u64, "{}", counted.count);
        assert!(
            counted.count <= PACKAGE_MAX_FILE_BYTES + 1,
            "{}",
            counted.count
        );
    }

    // Catches: trusting the header's size, or letting the zip crate stop at
    // it with an error that isn't the rule.
    #[test]
    fn counts_unpacked_bytes_not_headers() {
        let stream = deflated_zeros(21);
        let liar = Raw {
            name: "assets/big.wav",
            method: DEFLATED,
            flags: 0,
            crc: 0,
            packed: &stream,
            claimed_size: 10,
        };
        let zip = raw_zip(&[stored("manifest.lua", MANIFEST), liar]);
        assert_eq!(refused(&zip), "assets/big.wav is larger than 20 MB");
    }

    #[test]
    fn refuses_suspicious_compression() {
        let quiet = vec![0u8; 2 * MIB];
        let zip = zip_of(&[("manifest.lua", MANIFEST), ("assets/quiet.wav", &quiet)]);
        assert_eq!(
            refused(&zip),
            "assets/quiet.wav is compressed suspiciously well"
        );

        // Up to 1 MiB unpacked, the ratio isn't checked.
        let small = vec![0u8; MIB];
        let zip = zip_of(&[("manifest.lua", MANIFEST), ("assets/small.wav", &small)]);
        assert_eq!(
            Package::read_zip(&zip).unwrap().get("assets/small.wav"),
            Some(&small[..])
        );
    }

    #[test]
    fn refuses_too_many_files() {
        let names: Vec<String> = (1..PACKAGE_MAX_FILES)
            .map(|i| format!("server/{i}.lua"))
            .collect();
        let mut files: Vec<(&str, &[u8])> = vec![("manifest.lua", MANIFEST)];
        files.extend(names.iter().map(|name| (name.as_str(), &b"x"[..])));
        assert_eq!(files.len(), PACKAGE_MAX_FILES);
        assert!(Package::read_zip(&zip_of(&files)).is_ok());

        files.push(("server/one-more.lua", b"x"));
        assert_eq!(
            refused(&zip_of(&files)),
            "the plugin has more than 2000 files"
        );
    }

    #[test]
    fn refuses_a_plugin_too_large_unpacked() {
        let mut package = Package::read_zip(&zip_of(&[("manifest.lua", MANIFEST)])).unwrap();
        let file = PACKAGE_MAX_FILE_BYTES as usize;
        let error = package
            .insert("assets/big.wav", vec![0; file + 1])
            .unwrap_err();
        assert_eq!(error.to_string(), "assets/big.wav is larger than 20 MB");

        for i in 0..9 {
            package
                .insert(&format!("assets/{i}.wav"), vec![0; file])
                .unwrap();
        }
        // Ten full files and the manifest are over 200 MiB.
        let error = package.insert("assets/9.wav", vec![0; file]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the plugin is larger than 200 MB unpacked"
        );
        assert!(package.get("assets/9.wav").is_none());
        assert_eq!(
            package.total_bytes(),
            9 * PACKAGE_MAX_FILE_BYTES + MANIFEST.len() as u64
        );
    }

    #[test]
    fn needs_a_manifest_and_a_zip() {
        let zip = zip_of(&[("server/main.lua", b"x")]);
        assert_eq!(refused(&zip), "manifest.lua is missing");
        assert_eq!(refused(b"not a zip"), "this isn't a zip file");

        // An entry whose content doesn't match its checksum is damaged.
        let damaged = Raw {
            crc: 1,
            ..stored("server/x.lua", b"x")
        };
        let zip = raw_zip(&[stored("manifest.lua", MANIFEST), damaged]);
        assert_eq!(refused(&zip), "this isn't a zip file");
    }

    #[test]
    fn round_trips_through_zip() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer.add_directory("server/", options).unwrap();
        writer.add_directory("assets/", options).unwrap();
        for (path, bytes) in [
            ("manifest.lua", MANIFEST),
            ("server/main.luau", b"print('hi')"),
            ("assets/icon.png", &[0x89, b'P', b'N', b'G', 0, 255]),
        ] {
            writer.start_file(path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        let package = Package::read_zip(&writer.finish().unwrap().into_inner()).unwrap();
        assert_eq!(
            package.paths().collect::<Vec<_>>(),
            ["assets/icon.png", "manifest.lua", "server/main.luau"]
        );
        assert_eq!(package.get("server/main.luau"), Some(&b"print('hi')"[..]));
        assert_eq!(package.total_bytes(), (MANIFEST.len() + 11 + 6) as u64);

        let zip = package.write_zip();
        let again = Package::read_zip(&zip).unwrap();
        assert_eq!(again, package);
        // The same files always make the same zip.
        assert_eq!(again.write_zip(), zip);
    }

    #[test]
    fn from_dir_checks_the_same_rules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let write = |path: &str, bytes: &[u8]| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        write("server/main.luau", b"x");
        assert_eq!(
            Package::from_dir(root).unwrap_err().to_string(),
            "manifest.lua is missing"
        );

        write("manifest.lua", MANIFEST);
        write("assets/icon.png", b"png");
        write(".git/config", b"skipped");
        write("server/.notes.lua", b"skipped");
        write(".editorconfig", b"skipped");
        let package = Package::from_dir(root).unwrap();
        assert_eq!(
            package.paths().collect::<Vec<_>>(),
            ["assets/icon.png", "manifest.lua", "server/main.luau"]
        );

        write("README.md", b"x");
        assert_eq!(
            Package::from_dir(root).unwrap_err().to_string(),
            "README.md: files can only be in server/, client/ or assets/"
        );
        std::fs::remove_file(root.join("README.md")).unwrap();

        write(
            "assets/big.wav",
            &vec![0; PACKAGE_MAX_FILE_BYTES as usize + 1],
        );
        assert_eq!(
            Package::from_dir(root).unwrap_err().to_string(),
            "assets/big.wav is larger than 20 MB"
        );
        std::fs::remove_file(root.join("assets/big.wav")).unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/passwd", root.join("server/link.lua")).unwrap();
            assert_eq!(
                Package::from_dir(root).unwrap_err().to_string(),
                "server/link.lua: links aren't allowed"
            );
        }
    }
}
