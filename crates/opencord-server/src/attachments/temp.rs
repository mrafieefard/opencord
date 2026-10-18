//! Temporary encrypted file handling for in-flight uploads.

use std::io::{self, SeekFrom};
use std::path::Path;

use opencord_common::blob::{
    self, BlobKey, BlobKind, HEADER_LEN, SEGMENT_PLAIN, SEGMENT_SEALED, segment_count,
};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

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
