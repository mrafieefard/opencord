//! Channels' music on the node (Phase 3 plan §13.3, §18.4): the first
//! frame from the audio bridge starts a channel's music stream, announced
//! to everyone there with `MusicStream`; the main server stops it.

use std::time::Instant;

use opencord_proto::voice::v1 as voice;
use voice::envelope::Payload;

use super::Runtime;
use crate::sfu::MusicPacket;

impl Runtime {
    pub(super) fn music_frame(&mut self, now: Instant, channel_id: i64, packet: &MusicPacket) {
        if self.sfu.music_ssrc(channel_id).is_none() {
            let ssrc = self.free_ssrc(channel_id, &mut Vec::new());
            self.sfu.start_music(channel_id, ssrc);
            self.announce_music(now, channel_id, true);
        }
        self.sfu.forward_music(now, channel_id, packet);
    }

    pub(super) fn stop_music(&mut self, now: Instant, channel_id: i64) {
        if self.sfu.music_ssrc(channel_id).is_none() {
            return;
        }
        self.announce_music(now, channel_id, false);
        self.sfu.stop_music(channel_id);
    }

    /// What a client joining `channel_id` hears of its music, for Ready.
    pub(super) fn music_stream(&self, channel_id: i64) -> Option<voice::MusicStream> {
        self.sfu
            .music_ssrc(channel_id)
            .map(|ssrc| voice::MusicStream {
                channel_id,
                ssrc,
                active: true,
            })
    }

    fn announce_music(&mut self, now: Instant, channel_id: i64, active: bool) {
        let Some(ssrc) = self.sfu.music_ssrc(channel_id) else {
            return;
        };
        let stream = voice::MusicStream {
            channel_id,
            ssrc,
            active,
        };
        for session in self
            .sessions
            .values_mut()
            .filter(|session| session.channel_id == channel_id)
        {
            session.send_sequenced(Payload::MusicStream(stream), now);
        }
    }
}
