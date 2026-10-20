//! Temporary encrypted file handling for in-flight uploads.

use std::io::{self, SeekFrom};
use std::path::Path;

use opencord_common::blob::{
    self, BlobKey, BlobKind, HEADER_LEN, SEGMENT_OVERHEAD, SEGMENT_PLAIN, SEGMENT_SEALED,
    segment_count,
};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

pub async fn create(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await?;
    file.write_all(&blob::header()).await?;
    file.sync_data().await?;
    Ok(())
}

pub async fn append_piece(
    path: &Path,
    key: &BlobKey,
    id: i64,
    offset: u64,
    total: u64,
    piece: &[u8],
) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(path).await?;
    let truncate_target = HEADER_LEN + (offset / SEGMENT_PLAIN) * SEGMENT_SEALED;
    file.set_len(truncate_target).await?;
    file.seek(SeekFrom::Start(truncate_target)).await?;

    if total == 0 {
        let sealed = blob::seal_segment(key, BlobKind::File, id, 0, true, &[]);
        file.write_all(&sealed).await?;
    } else {
        let total_segments = segment_count(total);
        let base_index = offset / SEGMENT_PLAIN;
        for (i, chunk) in piece.chunks(SEGMENT_PLAIN as usize).enumerate() {
            let index = base_index + i as u64;
            let last = index == total_segments - 1;
            let sealed = blob::seal_segment(key, BlobKind::File, id, index, last, chunk);
            file.write_all(&sealed).await?;
        }
    }
    file.sync_data().await?;
    Ok(())
}

/// `len` bytes from `offset` of a complete upload of `total` bytes,
/// opening only the segments they lie in.
pub async fn read_range(
    path: &Path,
    key: &BlobKey,
    id: i64,
    total: u64,
    offset: u64,
    len: u64,
) -> io::Result<Vec<u8>> {
    let end = offset
        .checked_add(len)
        .filter(|&end| end <= total)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "read past the end"))?;
    let mut plain = Vec::with_capacity(len as usize);
    if len == 0 {
        return Ok(plain);
    }
    let mut file = tokio::fs::File::open(path).await?;
    let last = segment_count(total) - 1;
    for index in offset / SEGMENT_PLAIN..=(end - 1) / SEGMENT_PLAIN {
        let start = index * SEGMENT_PLAIN;
        let segment_len = (total - start).min(SEGMENT_PLAIN);
        let mut sealed = vec![0u8; (segment_len + SEGMENT_OVERHEAD) as usize];
        file.seek(SeekFrom::Start(HEADER_LEN + index * SEGMENT_SEALED))
            .await?;
        file.read_exact(&mut sealed).await?;
        let segment = blob::open_segment(key, BlobKind::File, id, index, index == last, &sealed)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let from = offset.saturating_sub(start) as usize;
        let to = (end - start).min(segment_len) as usize;
        plain.extend_from_slice(&segment[from..to]);
    }
    Ok(plain)
}

pub async fn read_plain(
    path: &Path,
    key: &BlobKey,
    id: i64,
    total: u64,
    limit: u64,
) -> io::Result<Vec<u8>> {
    if total > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "file exceeds limit",
        ));
    }
    let sealed = tokio::fs::read(path).await?;
    blob::open_all(key, BlobKind::File, id, &sealed)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_range_reads_across_segments_and_to_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("upload");
        let key = BlobKey::random();
        let total = 3 * SEGMENT_PLAIN + 100;
        let plain: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
        create(&path).await.unwrap();
        append_piece(&path, &key, 7, 0, total, &plain)
            .await
            .unwrap();

        for (offset, len) in [
            (0, 16),
            (SEGMENT_PLAIN - 5, 10),
            (10, 2 * SEGMENT_PLAIN),
            (total - 100, 100),
            (total, 0),
        ] {
            let read = read_range(&path, &key, 7, total, offset, len)
                .await
                .unwrap();
            assert_eq!(read, &plain[offset as usize..(offset + len) as usize]);
        }
        assert!(
            read_range(&path, &key, 7, total, total - 1, 2)
                .await
                .is_err()
        );
        assert!(read_range(&path, &key, 8, total, 0, 1).await.is_err());
    }
}
