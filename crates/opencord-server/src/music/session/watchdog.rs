//! The audio watchdog (Phase 3 plan §12.5): a channel that plays, whose
//! Lavalink signed in at the bridge but sends no audio for 2 s, gets its
//! node checked. (A Lavalink that never reaches the bridge is a setup
//! problem the connection test explains.) A node that doesn't answer within 2 s is unavailable, and the
//! channel moves; one that answers only gets that player restarted.

use std::time::{Duration, Instant};

use opencord_voice::bridge::{BridgeEvent, SILENT_AFTER};

use super::Session;

/// How long the node has to answer `GET /v4/info`.
const CHECK_WITHIN: Duration = Duration::from_secs(2);
/// A player is restarted at most this often; more silence after that is
/// left to the node's own health checks.
const RESTART_EVERY: Duration = Duration::from_secs(30);

/// What the bridge says about a channel's Lavalink connection.
#[derive(Debug, Default)]
pub(super) struct Watch {
    /// Lavalink signed in at the bridge for the current player.
    connected: bool,
    flowing: bool,
    /// Audio should have come by then.
    due: Option<Instant>,
    restarted_at: Option<Instant>,
}

impl Session {
    pub(super) fn bridge(&mut self, event: &BridgeEvent) {
        let playing = self.playing();
        let watch = &mut self.watch;
        match event {
            BridgeEvent::Connected { .. } | BridgeEvent::Discovered { .. } => {
                watch.connected = true;
            }
            BridgeEvent::Closed { .. } => {
                watch.connected = false;
                watch.flowing = false;
            }
            BridgeEvent::Audio { .. } => {
                watch.flowing = true;
                watch.due = None;
            }
            BridgeEvent::Silent { .. } => {
                watch.flowing = false;
                // Silent comes 2 s after the last packet already.
                if playing {
                    watch.due = Some(Instant::now());
                }
            }
            BridgeEvent::Refused { .. } => {}
        }
    }

    /// Music should be heard from now on: a track started or resumed.
    pub(super) fn expect_audio(&mut self) {
        if !self.watch.flowing {
            self.watch.due = Some(Instant::now() + SILENT_AFTER);
        }
    }

    pub(super) fn watchdog_due(&self) -> Option<Instant> {
        self.watch.due.filter(|_| self.watch.connected)
    }

    /// Checks the node when audio is overdue.
    pub(super) async fn watchdog(&mut self, now: Instant) {
        if self.watch.due.is_none_or(|due| due > now) {
            return;
        }
        // Not signed in at the bridge yet: checked once Lavalink is.
        if !self.watch.connected && self.playing() {
            return;
        }
        self.watch.due = None;
        if !self.playing() || self.watch.flowing {
            return;
        }
        let Some(player) = &self.player else {
            return;
        };
        let node_id = player.node_id;
        let Some(node) = self.state.lavalink.node(node_id) else {
            return;
        };
        let answered = match node.rest(&self.state) {
            Some((rest, _)) => matches!(
                tokio::time::timeout(CHECK_WITHIN, rest.info()).await,
                Ok(Ok(_))
            ),
            None => false,
        };
        let channel_id = self.channel_id;
        if !answered {
            tracing::warn!(
                channel_id,
                node_id,
                "no music from Lavalink, and it doesn't answer"
            );
            self.state.lavalink.unresponsive(
                &self.state,
                node_id,
                "Lavalink stopped sending audio and doesn't answer",
            );
            // Its channels move when its health changes; this one now.
            let position = self.position();
            self.drop_player();
            self.start_current(position, &[node_id]).await;
            return;
        }
        if self
            .watch
            .restarted_at
            .is_some_and(|at| now.duration_since(at) < RESTART_EVERY)
        {
            tracing::warn!(
                channel_id,
                node_id,
                "Lavalink answers but still sends no music"
            );
            return;
        }
        tracing::warn!(
            channel_id,
            node_id,
            "no music from Lavalink: restarting its player"
        );
        self.watch.restarted_at = Some(now);
        let position = self.position();
        let Some(item) = self.queue.now.clone() else {
            return;
        };
        let paused = self.paused || self.paused_empty;
        let restarted = match self.player.as_mut() {
            Some(player) => {
                player.renew_voice();
                player
                    .play(&self.state, channel_id, &item, position, paused)
                    .await
                    .is_ok()
            }
            None => false,
        };
        if restarted {
            self.set_position(position);
            self.expect_audio();
        } else {
            self.drop_player();
            self.start_current(position, &[]).await;
        }
    }
}
