//! Unpacks a release archive: `.tar.gz` on Linux, `.zip` on Windows and
//! macOS. Entries that would land outside the folder are refused.

use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

/// Unpacks `archive`, named `name` in the release, into `dest`.
pub fn unpack(archive: &Path, name: &str, dest: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dest)?;
    let file = BufReader::new(File::open(archive)?);
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        tar.set_preserve_permissions(true);
        tar.set_overwrite(true);
        for entry in tar.entries()? {
            let mut entry = entry?;
            // `unpack_in` refuses paths with `..` or a root, and links
            // that would write outside `dest`.
            if !entry.unpack_in(dest)? {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} would land outside the folder", entry.path()?.display()),
                ));
            }
        }
        Ok(())
    } else if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(file).map_err(io::Error::other)?;
        // Refuses entries outside `dest`; keeps Unix modes and symlinks,
        // which a macOS app's frameworks are full of.
        zip.extract(dest).map_err(io::Error::other)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} is not an archive the launcher unpacks"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn tar_gz(path: &Path, entries: &[(&str, &[u8], u32)]) {
        let file = File::create(path).unwrap();
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::fast(),
        ));
        for (name, data, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(*mode);
            header.set_entry_type(tar::EntryType::Regular);
            // `append_data` refuses `..`, so the name goes in by hand.
            header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            builder.append(&header, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn a_tarball_unpacks_with_its_modes() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        tar_gz(
            &archive,
            &[
                ("opencord/opencord", b"exe", 0o755),
                ("opencord/data/x", b"x", 0o644),
            ],
        );
        let dest = dir.path().join("out");

        unpack(&archive, "opencord-linux-x64.tar.gz", &dest).unwrap();

        assert_eq!(
            std::fs::read(dest.join("opencord/opencord")).unwrap(),
            b"exe"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dest.join("opencord/opencord"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }
    }

    #[test]
    fn a_tarball_reaching_outside_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        tar_gz(&archive, &[("../escaped", b"x", 0o644)]);
        let dest = dir.path().join("out");

        assert!(unpack(&archive, "a.tar.gz", &dest).is_err());
        assert!(!dir.path().join("escaped").exists());
    }

    #[test]
    fn a_zip_unpacks() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file("opencord/opencord.exe", options).unwrap();
        zip.write_all(b"exe").unwrap();
        zip.finish().unwrap();
        let dest = dir.path().join("out");

        unpack(&archive, "opencord-windows-x64.zip", &dest).unwrap();

        assert_eq!(
            std::fs::read(dest.join("opencord/opencord.exe")).unwrap(),
            b"exe"
        );
    }

    #[test]
    fn other_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.dmg");
        std::fs::write(&archive, b"x").unwrap();
        assert!(unpack(&archive, "a.dmg", &dir.path().join("out")).is_err());
    }
}
