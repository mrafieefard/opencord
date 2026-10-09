//! The ScreenCast portal (plan §9.1, §9.3): the system picker chooses a
//! screen or a window after the user chose quality and audio. Sharing
//! persists until revoked, and its restore token lets "Share the same
//! screen again" skip the picker next time.

use std::os::fd::OwnedFd;

use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use ashpd::desktop::{PersistMode, ResponseError, Session};

use super::{ScreenError, SourceKind};

/// A screen or window the user picked; sharing ends when this is dropped
/// (the portal session closes with it).
pub struct Picked {
    /// The PipeWire node to read, on [`Picked::remote`].
    pub node_id: u32,
    pub remote: OwnedFd,
    pub kind: SourceKind,
    /// As the compositor reports it, which may not be pixels.
    pub size: Option<(u32, u32)>,
    /// For the next share of the same source, without the picker.
    pub restore_token: Option<String>,
    session: Session<Screencast>,
}

impl Picked {
    /// Ends the portal session now (dropping it ends it too, later).
    pub async fn close(&self) {
        let _ = self.session.close().await;
    }
}

fn failed(error: ashpd::Error) -> ScreenError {
    match error {
        ashpd::Error::Response(ResponseError::Cancelled) => ScreenError::Cancelled,
        ashpd::Error::Response(ResponseError::Other) => ScreenError::Denied,
        ashpd::Error::PortalNotFound(_) => ScreenError::NotSupported,
        other => ScreenError::Failed(other.to_string()),
    }
}

/// Asks the portal for a screen or a window: its picker, unless
/// `restore_token` names an earlier choice it still allows.
pub async fn pick(restore_token: Option<&str>) -> Result<Picked, ScreenError> {
    let proxy = Screencast::new().await.map_err(failed)?;
    let available = proxy
        .available_source_types()
        .await
        .unwrap_or(SourceType::Monitor.into());
    // Window sharing depends on the portal backend (plan §9.3).
    let types = available & (SourceType::Monitor | SourceType::Window);
    let cursor = match proxy.available_cursor_modes().await {
        Ok(modes) if modes.contains(CursorMode::Embedded) => CursorMode::Embedded,
        _ => CursorMode::Hidden,
    };
    let session = proxy
        .create_session(Default::default())
        .await
        .map_err(failed)?;
    proxy
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                .set_cursor_mode(cursor)
                .set_sources(types)
                .set_multiple(false)
                .set_persist_mode(PersistMode::ExplicitlyRevoked)
                .set_restore_token(restore_token),
        )
        .await
        .map_err(failed)?
        .response()
        .map_err(failed)?;
    let streams = proxy
        .start(&session, None, Default::default())
        .await
        .map_err(failed)?
        .response()
        .map_err(failed)?;
    let stream = streams.streams().first().ok_or(ScreenError::Cancelled)?;
    let kind = match stream.source_type() {
        Some(SourceType::Window) => SourceKind::Window,
        _ => SourceKind::Screen,
    };
    let size = stream.size().and_then(|(width, height)| {
        Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
    });
    let node_id = stream.pipe_wire_node_id();
    let restore_token = streams.restore_token().map(str::to_owned);
    let remote = proxy
        .open_pipe_wire_remote(&session, Default::default())
        .await
        .map_err(failed)?;
    Ok(Picked {
        node_id,
        remote,
        kind,
        size,
        restore_token,
        session,
    })
}
