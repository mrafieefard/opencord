//! Linux's screens: the ScreenCast portal's system picker and its PipeWire
//! node, or a node named in `OPENCORD_SCREEN_NODE` (checks that run with
//! nobody to pick), read from PipeWire's own session.

use std::os::fd::OwnedFd;
use std::sync::mpsc::SyncSender;

use super::capture::{ScreenCapture, Target};
use super::{Picked, ScreenError, ScreenSource, ScreenSupport, SourceKind, StartRequest, portal};
use crate::video::camera::RawFrame;
use crate::video::camera::pipewire::Remote;

/// Names a PipeWire node to share in place of the portal's picker.
const NODE_VARIABLE: &str = "OPENCORD_SCREEN_NODE";

pub(super) struct Capture {
    capture: ScreenCapture,
    /// The portal session: sharing ends when it goes.
    _picked: Option<portal::Picked>,
}

impl Capture {
    pub(super) fn size(&self) -> (u32, u32) {
        self.capture.size
    }

    pub(super) fn finished(&self) -> bool {
        self.capture.finished()
    }
}

/// The portal is asked when sharing starts; whether it is there shows then.
pub(super) fn support() -> ScreenSupport {
    ScreenSupport {
        supported: true,
        reason: None,
        own_picker: false,
    }
}

pub(super) async fn sources() -> Result<Vec<ScreenSource>, ScreenError> {
    Ok(Vec::new())
}

/// The portal's picker, unless `restore_token` still names a source.
pub(super) async fn start(
    request: StartRequest,
    frames: SyncSender<RawFrame>,
) -> Result<(Capture, Picked), ScreenError> {
    let (remote, target, picked) = match std::env::var(NODE_VARIABLE) {
        Ok(name) => (Remote::Session, Target::Named(name), None),
        Err(_) => {
            let picked = portal::pick(request.restore_token.as_deref()).await?;
            let remote: OwnedFd = picked
                .remote
                .try_clone()
                .map_err(|error| ScreenError::Failed(error.to_string()))?;
            (
                Remote::Portal(remote),
                Target::Id(picked.node_id),
                Some(picked),
            )
        }
    };
    let fps = request.fps;
    let capture =
        tokio::task::spawn_blocking(move || ScreenCapture::start(remote, target, fps, frames))
            .await
            .map_err(|error| ScreenError::Failed(error.to_string()))??;
    let chosen = Picked {
        kind: picked
            .as_ref()
            .map_or(SourceKind::Screen, |picked| picked.kind),
        restore_token: picked
            .as_ref()
            .and_then(|picked| picked.restore_token.clone()),
        // The portal says nothing of which app a window is (plan §10.4).
        sound: None,
    };
    Ok((
        Capture {
            capture,
            _picked: picked,
        },
        chosen,
    ))
}
