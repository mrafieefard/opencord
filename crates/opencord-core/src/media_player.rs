//! Plays video and audio attachments (Milestone A7, Task 34): the media
//! engine reads the attachment over HTTP ranges, shows pictures on a Flutter
//! texture, and plays sound into the call or through the speaker.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use opencord_media::player::{AudioOut, MediaPlayer, PlayerEvent, VideoOut};
use tokio::sync::mpsc;

use crate::api::types::{CoreError, MediaEvent, MediaHandle, PlaybackItem};
use crate::attachments::range_reader::RangeReader;
use crate::client::Client;
use crate::now_playing::{NowPlaying, Player, Stoppable};
use crate::output_sink::{CallMix, OutputSink};
use crate::video::FrameSink;

struct TextureVideoOut {
    sink: Arc<dyn FrameSink>,
}

impl VideoOut for TextureVideoOut {
    fn present(&self, width: u32, height: u32, rgba: Vec<u8>) {
        self.sink.present(width, height, rgba);
    }
}

/// What the app last heard about the open media, kept for the events the
/// core sends itself (play, pause, seek, buffering).
struct Status {
    item: PlaybackItem,
    events: mpsc::UnboundedSender<MediaEvent>,
    playing: AtomicBool,
    position_ms: AtomicU32,
    duration_ms: AtomicU32,
    buffering: AtomicBool,
}

impl Status {
    fn send(&self) {
        let _ = self.events.send(MediaEvent::PlaybackUpdate {
            item: self.item.clone(),
            position_ms: self.position_ms.load(Ordering::Relaxed),
            duration_ms: self.duration_ms.load(Ordering::Relaxed),
            playing: self.playing.load(Ordering::Relaxed),
            buffering: self.buffering.load(Ordering::Relaxed),
        });
    }
}

struct ActiveMedia {
    player: MediaPlayer,
    status: Arc<Status>,
}

/// Pauses the media when something else starts playing.
struct PauseOnStop(Arc<CoreMediaPlayer>);

impl Stoppable for PauseOnStop {
    fn stop(&self) {
        self.0.pause();
    }
}

/// One media attachment open at a time.
#[derive(Default)]
pub(crate) struct CoreMediaPlayer {
    active: Mutex<Option<ActiveMedia>>,
}

impl CoreMediaPlayer {
    pub async fn open(
        &self,
        client: &Client,
        server_key: String,
        attachment_id: i64,
    ) -> Result<MediaHandle, CoreError> {
        if !opencord_media::player::supported() {
            return Err(CoreError::InvalidInput {
                message: "This computer can't play media yet".to_owned(),
            });
        }
        if let Some(previous) = self.take() {
            previous.player.close();
        }

        let mut reader =
            RangeReader::open(client.clone(), server_key.clone(), attachment_id).await?;
        let media = client.media().ok_or(CoreError::NotInitialized)?;
        let texture = media.video().create_texture();
        let texture_id = texture.as_ref().map(|sink| sink.id());
        let video_out = texture.map(|sink| Arc::new(TextureVideoOut { sink }) as Arc<dyn VideoOut>);
        let call = media.call_engine().map(|e| e as Arc<dyn CallMix>);
        let audio_out: Box<dyn AudioOut> = Box::new(OutputSink::open(
            call,
            &media.audio_settings(),
            media.options().open_devices,
        )?);

        let status = Arc::new(Status {
            item: PlaybackItem::Attachment {
                server_key,
                attachment_id,
            },
            events: media.events().clone(),
            playing: AtomicBool::new(false),
            position_ms: AtomicU32::new(0),
            duration_ms: AtomicU32::new(0),
            buffering: AtomicBool::new(false),
        });
        let waiting = status.clone();
        reader.on_wait(move |buffering| {
            waiting.buffering.store(buffering, Ordering::Relaxed);
            waiting.send();
        });

        let (opened_tx, opened) = tokio::sync::oneshot::channel();
        let mut opened_tx = Some(opened_tx);
        let heard = status.clone();
        let events = move |event: PlayerEvent| match event {
            PlayerEvent::Opened {
                duration_ms,
                width,
                height,
                has_video,
                ..
            } => {
                heard.duration_ms.store(duration_ms, Ordering::Relaxed);
                if let Some(tx) = opened_tx.take() {
                    let _ = tx.send(Ok((duration_ms, width, height, has_video)));
                }
            }
            PlayerEvent::Position {
                position_ms,
                buffering,
            } => {
                heard.position_ms.store(position_ms, Ordering::Relaxed);
                heard.buffering.store(buffering, Ordering::Relaxed);
                heard.send();
            }
            PlayerEvent::Ended => {
                heard.playing.store(false, Ordering::Relaxed);
                let _ = heard.events.send(MediaEvent::PlaybackEnded {
                    item: heard.item.clone(),
                });
            }
            PlayerEvent::Failed(message) => {
                heard.playing.store(false, Ordering::Relaxed);
                match opened_tx.take() {
                    Some(tx) => {
                        let _ = tx.send(Err(message));
                    }
                    None => {
                        let _ = heard.events.send(MediaEvent::PlaybackFailed {
                            item: heard.item.clone(),
                            message,
                        });
                    }
                }
            }
        };

        let player =
            MediaPlayer::open(Box::new(reader), video_out, audio_out, events).map_err(|e| {
                CoreError::InvalidInput {
                    message: e.to_string(),
                }
            })?;
        let (duration_ms, width, height, has_video) = match opened.await {
            Ok(Ok(info)) => info,
            Ok(Err(message)) => return Err(CoreError::InvalidInput { message }),
            Err(_) => {
                return Err(CoreError::InvalidInput {
                    message: "the media player stopped before opening".to_owned(),
                });
            }
        };
        let replaced = self
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(ActiveMedia { player, status });
        if let Some(replaced) = replaced {
            replaced.player.close();
        }
        Ok(MediaHandle {
            texture_id: if has_video { texture_id } else { None },
            duration_ms,
            width,
            height,
            has_video,
        })
    }

    fn take(&self) -> Option<ActiveMedia> {
        self.active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Runs `action` on the open media, if any.
    fn with_active(&self, action: impl FnOnce(&ActiveMedia)) {
        if let Some(active) = &*self.active.lock().unwrap_or_else(PoisonError::into_inner) {
            action(active);
        }
    }

    pub fn play(self: &Arc<Self>, now_playing: &NowPlaying) {
        let mut opened = false;
        self.with_active(|active| {
            active.status.playing.store(true, Ordering::Relaxed);
            active.player.play();
            active.status.send();
            opened = true;
        });
        if opened {
            now_playing.claim(Player::Media, || Box::new(PauseOnStop(self.clone())));
        }
    }

    pub fn pause(&self) {
        self.with_active(|active| {
            active.status.playing.store(false, Ordering::Relaxed);
            active.player.pause();
            active.status.send();
        });
    }

    pub fn seek(&self, position_ms: u32) {
        self.with_active(|active| {
            active
                .status
                .position_ms
                .store(position_ms, Ordering::Relaxed);
            active.player.seek(position_ms);
        });
    }

    pub fn set_volume(&self, volume: f32, muted: bool) {
        self.with_active(|active| active.player.set_volume(volume, muted));
    }

    pub fn close(&self, now_playing: &NowPlaying) {
        if let Some(active) = self.take() {
            active.player.close();
        }
        now_playing.clear(Player::Media);
    }
}
