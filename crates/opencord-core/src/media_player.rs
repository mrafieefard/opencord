//! Core media player glue (Milestone A7, Task 34): connects RangeReader,
//! Flutter textures, OutputSink, and NowPlaying for video and audio attachment playback.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use opencord_media::player::{AudioOut, MediaPlayer, PlayerEvent, VideoOut};
use tokio::sync::mpsc;

use crate::api::types::{CoreError, MediaEvent, MediaHandle, PlaybackItem};
use crate::attachments::range_reader::RangeReader;
use crate::client::Client;
use crate::now_playing::{NowPlaying, Stoppable};
use crate::output_sink::OutputSink;
use crate::video::FrameSink;

struct TextureVideoOut {
    sink: Arc<dyn FrameSink>,
}

impl VideoOut for TextureVideoOut {
    fn present(&self, width: u32, height: u32, rgba: Vec<u8>) {
        self.sink.present(width, height, rgba);
    }
}

struct MediaPlayerHandle {
    media_player: Arc<CoreMediaPlayer>,
}

impl Stoppable for MediaPlayerHandle {
    fn stop(&self) {
        self.media_player.pause();
    }
}

struct ActiveMedia {
    player: MediaPlayer,
    is_playing: Arc<AtomicBool>,
    duration_ms: Arc<AtomicU32>,
    item: PlaybackItem,
    events: mpsc::UnboundedSender<MediaEvent>,
}

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
                message: "media player is not supported on this platform".to_owned(),
            });
        }

        // Close any currently active media
        {
            let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(a) = active.take() {
                a.player.close();
            }
        }

        let reader = RangeReader::open(client.clone(), server_key.clone(), attachment_id).await?;

        let media = client.media().ok_or(CoreError::NotInitialized)?;
        let texture_sink = media.video().create_texture();
        let raw_texture_id = texture_sink.as_ref().map(|s| s.id());
        let video_out: Option<Arc<dyn VideoOut>> =
            texture_sink.map(|sink| Arc::new(TextureVideoOut { sink }) as Arc<dyn VideoOut>);

        let engine = media.call_engine();
        let settings = media.audio_settings();
        let open_devices = media.options().open_devices;
        let audio_out: Box<dyn AudioOut> =
            Box::new(OutputSink::open(engine, &settings, open_devices)?);

        let (open_tx, open_rx) = tokio::sync::oneshot::channel();
        let mut open_tx = Some(open_tx);
        let is_playing = Arc::new(AtomicBool::new(false));
        let duration_ms = Arc::new(AtomicU32::new(0));
        let item = PlaybackItem::Attachment {
            server_key,
            attachment_id,
        };
        let events_out = media.events().clone();

        let item_clone = item.clone();
        let is_playing_flag = is_playing.clone();
        let duration_flag = duration_ms.clone();
        let events_callback = move |event: PlayerEvent| match event {
            PlayerEvent::Opened {
                duration_ms,
                width,
                height,
                has_video,
                ..
            } => {
                duration_flag.store(duration_ms, Ordering::Relaxed);
                if let Some(tx) = open_tx.take() {
                    let _ = tx.send(Ok((duration_ms, width, height, has_video)));
                }
            }
            PlayerEvent::Position {
                position_ms,
                buffering,
            } => {
                let playing = is_playing_flag.load(Ordering::Relaxed);
                let duration = duration_flag.load(Ordering::Relaxed);
                let _ = events_out.send(MediaEvent::PlaybackUpdate {
                    item: item_clone.clone(),
                    position_ms,
                    duration_ms: duration,
                    playing,
                    buffering,
                });
            }
            PlayerEvent::Ended => {
                is_playing_flag.store(false, Ordering::Relaxed);
                let _ = events_out.send(MediaEvent::PlaybackEnded {
                    item: item_clone.clone(),
                });
            }
            PlayerEvent::Failed(err) => {
                if let Some(tx) = open_tx.take() {
                    let _ = tx.send(Err(err.clone()));
                }
                let _ = events_out.send(MediaEvent::PlaybackFailed {
                    item: item_clone.clone(),
                    message: err,
                });
            }
        };

        let player = MediaPlayer::open(Box::new(reader), video_out, audio_out, events_callback)
            .map_err(|e| CoreError::InvalidInput {
                message: e.to_string(),
            })?;

        let (dur, w, h, has_video) = match open_rx.await {
            Ok(Ok(info)) => info,
            Ok(Err(err)) => return Err(CoreError::InvalidInput { message: err }),
            Err(_) => {
                return Err(CoreError::InvalidInput {
                    message: "media player thread terminated without opening".to_owned(),
                });
            }
        };

        let texture_id = if has_video { raw_texture_id } else { None };
        let handle = MediaHandle {
            texture_id,
            duration_ms: dur,
            width: w,
            height: h,
            has_video,
        };

        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        *active = Some(ActiveMedia {
            player,
            is_playing,
            duration_ms,
            item,
            events: media.events().clone(),
        });

        Ok(handle)
    }

    pub fn play(&self, now_playing: &NowPlaying, this: &Arc<CoreMediaPlayer>) {
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(a) = &mut *active {
            a.is_playing.store(true, Ordering::Relaxed);
            a.player.play();
            let _ = a.events.send(MediaEvent::PlaybackUpdate {
                item: a.item.clone(),
                position_ms: 0,
                duration_ms: a.duration_ms.load(Ordering::Relaxed),
                playing: true,
                buffering: false,
            });
            now_playing.replace(Box::new(MediaPlayerHandle {
                media_player: this.clone(),
            }));
        }
    }

    pub fn pause(&self) {
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(a) = &mut *active {
            a.is_playing.store(false, Ordering::Relaxed);
            a.player.pause();
            let _ = a.events.send(MediaEvent::PlaybackUpdate {
                item: a.item.clone(),
                position_ms: 0,
                duration_ms: a.duration_ms.load(Ordering::Relaxed),
                playing: false,
                buffering: false,
            });
        }
    }

    pub fn seek(&self, position_ms: u32) {
        let active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(a) = &*active {
            a.player.seek(position_ms);
            let _ = a.events.send(MediaEvent::PlaybackUpdate {
                item: a.item.clone(),
                position_ms,
                duration_ms: a.duration_ms.load(Ordering::Relaxed),
                playing: a.is_playing.load(Ordering::Relaxed),
                buffering: false,
            });
        }
    }

    pub fn set_volume(&self, volume: f32, muted: bool) {
        let active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(a) = &*active {
            a.player.set_volume(volume, muted);
        }
    }

    pub fn close(&self, now_playing: &NowPlaying) {
        let active = self
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(a) = active {
            a.player.close();
        }
        now_playing.clear();
    }
}
