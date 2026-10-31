//! Attachments as files on this device (design §5.6).

use crate::MobileError;
use crate::convert;
use crate::files::{self, cached, path_text, storage};
use crate::ids::parse_id;
use crate::mobile::mobile;
use crate::types::AttachmentVariant;

/// The path of an attachment's file in the cache, fetched from the server
/// unless it is there: a thumbnail whole, an original streamed to disk.
#[uniffi::export]
pub async fn attachment_file(
    server_key: String,
    attachment_id: String,
    variant: AttachmentVariant,
) -> Result<String, MobileError> {
    let attachment_id = parse_id(&attachment_id)?;
    let mobile = mobile()?;
    let target = files::file_path(mobile.cache_dir(), &server_key, attachment_id, variant);
    let path = mobile
        .run(move |client| async move {
            cached(target, |part| async move {
                match variant {
                    AttachmentVariant::Thumbnail => {
                        let bytes = client
                            .attachment_bytes(&server_key, attachment_id, convert::variant(variant))
                            .await?;
                        tokio::fs::write(&part, bytes)
                            .await
                            .map_err(|error| storage(&error))
                    }
                    AttachmentVariant::Original => {
                        let part = path_text(part)?;
                        Ok(client
                            .download_attachment(&server_key, attachment_id, part)
                            .await?)
                    }
                }
            })
            .await
        })
        .await?;
    path_text(path)
}
