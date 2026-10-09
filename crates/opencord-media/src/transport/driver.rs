//! The task behind a [`super::VoiceConnection`]: the voice gateway and the
//! media connection, resuming the gateway when it drops.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use opencord_common::voice::close;
use opencord_proto::voice::v1 as voice;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use voice::envelope::Payload;

use super::gateway::{self, GatewayUrl, Next, Socket};
use super::media::{Media, remote_track};
use super::{
    Command, NETWORK_CHECK, RESUME_FOR, RESUME_PAUSE, SinkWant, TrackError, TrackKind,
    TrackRequest, TransportError, VoiceEvent, hello, unix_ms,
};

/// `MediaSinkWants` go out at most this often (the node allows 20 a
/// second); the latest wins.
const WANTS_EVERY: Duration = Duration::from_millis(100);
/// Datagrams read in one go, each stamped as it is read, before any is
/// processed.
const READ_BATCH: usize = 32;
const DATAGRAM: usize = 2048;

type Reconnect = JoinHandle<Result<Socket, Option<u16>>>;

/// What the loop does after a command.
enum Flow {
    Continue,
    Stop,
}

pub(super) struct Driver {
    url: GatewayUrl,
    fingerprint: Option<[u8; 32]>,
    voice_session_id: String,
    resume_token: Vec<u8>,
    heartbeat: Duration,
    /// A heartbeat went out and its ack has not come back.
    awaiting_ack: bool,
    last_seq: u64,
    media: Media,
    events: mpsc::UnboundedSender<VoiceEvent>,
    /// Publish requests waiting for the node's answer, by track id.
    publishing: HashMap<String, (TrackRequest, oneshot::Sender<Result<(), TrackError>>)>,
    /// Wants not sent yet, and when the last went out.
    wants_pending: Option<Vec<SinkWant>>,
    wants_sent: Option<Instant>,
}

impl Driver {
    pub fn new(
        url: GatewayUrl,
        fingerprint: Option<[u8; 32]>,
        ready: &voice::Ready,
        heartbeat: Duration,
        media: Media,
        events: mpsc::UnboundedSender<VoiceEvent>,
    ) -> Self {
        Self {
            url,
            fingerprint,
            voice_session_id: ready.voice_session_id.clone(),
            resume_token: ready.resume_token.clone(),
            heartbeat,
            awaiting_ack: false,
            last_seq: 0,
            media,
            events,
            publishing: HashMap::new(),
            wants_pending: None,
            wants_sent: None,
        }
    }

    pub async fn run(mut self, socket: Socket, mut commands: mpsc::UnboundedReceiver<Command>) {
        let mut socket = Some(socket);
        let mut reconnect: Option<Reconnect> = None;
        let mut batch = vec![[0u8; DATAGRAM]; READ_BATCH];
        let mut lengths = [(0usize, Instant::now()); READ_BATCH];
        let mut beat = tokio::time::interval(self.heartbeat);
        beat.tick().await;
        let mut network = tokio::time::interval(NETWORK_CHECK);
        network.tick().await;
        let mut nonce = 0u64;
        self.media.drain(&self.events);
        loop {
            let wake = self.media.timeout.max(Instant::now());
            tokio::select! {
                received = self.media.socket.recv(&mut batch[0]) => {
                    if let Ok(size) = received {
                        lengths[0] = (size, Instant::now());
                        let mut count = 1;
                        while count < READ_BATCH {
                            let Ok(size) = self.media.socket.try_recv(&mut batch[count]) else {
                                break;
                            };
                            lengths[count] = (size, Instant::now());
                            count += 1;
                        }
                        for (datagram, (size, at)) in batch.iter().zip(&lengths).take(count) {
                            self.media.receive(*at, &datagram[..*size], &self.events);
                        }
                    }
                }
                () = tokio::time::sleep_until(wake.into()) => self.media.timeout(),
                () = sleep_until_maybe(self.media.shims_due()) => {
                    self.media.release_shims(&self.events);
                }
                () = sleep_until_maybe(self.wants_due()) => {
                    if let Some(open) = socket.as_mut() {
                        self.send_wants(open).await;
                    }
                }
                next = next_or_pending(socket.as_mut()) => match next {
                    Next::Envelope(envelope) => self.on_envelope(envelope),
                    Next::Closed(code) => {
                        socket = None;
                        if !close::is_resumable(code) {
                            self.finish(code);
                            return;
                        }
                        reconnect = Some(self.resume_later());
                    }
                },
                done = join_or_pending(reconnect.as_mut()) => {
                    reconnect = None;
                    match done {
                        Ok(Ok(resumed)) => {
                            socket = Some(resumed);
                            self.awaiting_ack = false;
                        }
                        Ok(Err(code)) => {
                            self.finish(code.or(Some(close::SESSION_INVALID)));
                            return;
                        }
                        Err(_) => {
                            self.finish(None);
                            return;
                        }
                    }
                }
                _ = network.tick() => {
                    if self.media.network_changed() {
                        self.rebind().await;
                    }
                    self.media.tick(Instant::now(), &self.events);
                }
                _ = beat.tick() => {
                    if let Some(open) = socket.as_mut() {
                        if self.awaiting_ack {
                            // The gateway went quiet: treat it as dropped.
                            socket = None;
                            reconnect = Some(self.resume_later());
                        } else {
                            nonce += 1;
                            self.awaiting_ack = true;
                            let beat = Payload::Heartbeat(voice::Heartbeat {
                                nonce,
                                client_ts_ms: unix_ms(),
                            });
                            if gateway::send(open, beat).await.is_err() {
                                socket = None;
                                reconnect = Some(self.resume_later());
                            }
                        }
                    }
                }
                command = commands.recv() => {
                    let flow = self.on_command(command, &mut socket, &mut reconnect).await;
                    if let Flow::Stop = flow {
                        return;
                    }
                }
            }
            self.media.drain(&self.events);
        }
    }

    async fn on_command(
        &mut self,
        command: Option<Command>,
        socket: &mut Option<Socket>,
        reconnect: &mut Option<Reconnect>,
    ) -> Flow {
        match command {
            Some(Command::Audio(frame)) => self.media.send_audio(frame),
            Some(Command::Video(frame)) => self.media.send_video(frame),
            Some(Command::Keyframe(track_id)) => self.media.request_keyframe(&track_id),
            Some(Command::Speaking(flags)) => {
                if let Some(open) = socket.as_mut() {
                    let speaking = Payload::Speaking(voice::Speaking { flags, user_id: 0 });
                    let _ = gateway::send(open, speaking).await;
                }
            }
            Some(Command::Publish { request, reply }) => {
                self.publish(socket.as_mut(), request, reply).await;
            }
            Some(Command::Unpublish(track_id)) => {
                self.media.unpublish(&track_id);
                if let Some(open) = socket.as_mut() {
                    let unpublish = Payload::UnpublishTrack(voice::UnpublishTrack { track_id });
                    let _ = gateway::send(open, unpublish).await;
                }
            }
            Some(Command::SinkWants(wants)) => {
                self.wants_pending = Some(wants);
                if self.wants_due().is_some_and(|due| due <= Instant::now())
                    && let Some(open) = socket.as_mut()
                {
                    self.send_wants(open).await;
                }
            }
            #[cfg(any(test, feature = "testing"))]
            Some(Command::Impair { inbound, outbound }) => {
                self.media.set_impairment(&inbound, &outbound);
            }
            Some(Command::DropGateway) => {
                *socket = None;
                *reconnect = Some(self.resume_later());
            }
            Some(Command::Rebind) => self.rebind().await,
            Some(Command::Close) | None => {
                self.media.rtc.disconnect();
                self.media.drain(&self.events);
                if let Some(mut open) = socket.take() {
                    let _ = open.close(None).await;
                }
                if let Some(pending) = reconnect.take() {
                    pending.abort();
                }
                self.finish(None);
                return Flow::Stop;
            }
        }
        Flow::Continue
    }

    async fn publish(
        &mut self,
        socket: Option<&mut Socket>,
        request: TrackRequest,
        reply: oneshot::Sender<Result<(), TrackError>>,
    ) {
        let Some(open) = socket else {
            let _ = reply.send(Err(TrackError::NotConnected));
            return;
        };
        let publish = voice::PublishTrack {
            track_id: request.track_id.clone(),
            kind: match request.kind {
                TrackKind::Camera => voice::TrackKind::Camera,
                TrackKind::Screen => voice::TrackKind::Screen,
            } as i32,
            codec: voice::Codec::H264 as i32,
            layers: request
                .layers
                .iter()
                .map(|layer| voice::Layer {
                    rid: layer.rid.clone(),
                    width: layer.width,
                    height: layer.height,
                    fps: layer.fps,
                    max_bitrate: layer.max_bitrate,
                })
                .collect(),
        };
        if gateway::send(open, Payload::PublishTrack(publish))
            .await
            .is_err()
        {
            let _ = reply.send(Err(TrackError::NotConnected));
            return;
        }
        self.publishing
            .insert(request.track_id.clone(), (request, reply));
    }

    /// When pending wants may go out.
    fn wants_due(&self) -> Option<Instant> {
        self.wants_pending.as_ref()?;
        Some(
            self.wants_sent
                .map_or_else(Instant::now, |sent| sent + WANTS_EVERY),
        )
    }

    async fn send_wants(&mut self, socket: &mut Socket) {
        let Some(wants) = self.wants_pending.take() else {
            return;
        };
        self.wants_sent = Some(Instant::now());
        let message = voice::MediaSinkWants {
            wants: wants
                .into_iter()
                .map(|want| voice::SinkWant {
                    track_id: want.track_id,
                    max_height: want.max_height,
                    max_fps: 0,
                    paused: false,
                })
                .collect(),
        };
        let _ = gateway::send(socket, Payload::MediaSinkWants(message)).await;
    }

    async fn rebind(&mut self) {
        if let Err(error) = self.media.rebind().await {
            tracing::debug!(%error, "could not move media to the new network yet");
        }
    }

    fn on_envelope(&mut self, envelope: Box<voice::Envelope>) {
        if envelope.seq > 0 {
            self.last_seq = self.last_seq.max(envelope.seq);
        }
        match envelope.payload {
            Some(Payload::HeartbeatAck(_)) => self.awaiting_ack = false,
            Some(Payload::ClientConnect(connect)) => {
                self.media.hear(connect.user_id, connect.audio_ssrc);
                let _ = self.events.send(VoiceEvent::ClientConnected {
                    user_id: connect.user_id,
                    audio_ssrc: connect.audio_ssrc,
                });
            }
            Some(Payload::ClientDisconnect(gone)) => {
                self.media.forget(gone.user_id);
                let _ = self.events.send(VoiceEvent::ClientDisconnected {
                    user_id: gone.user_id,
                });
            }
            Some(Payload::Speaking(speaking)) => {
                let _ = self.events.send(VoiceEvent::Speaking {
                    user_id: speaking.user_id,
                    flags: speaking.flags,
                });
            }
            Some(Payload::TrackPublished(published)) => {
                if let Some((request, reply)) = self.publishing.remove(&published.track_id) {
                    self.media
                        .publish(&request, &published.layers, &self.events);
                    let _ = reply.send(Ok(()));
                }
            }
            Some(Payload::TrackRejected(rejected)) => self.on_rejected(rejected),
            Some(Payload::TrackUpdate(update)) => self.on_track_update(update),
            Some(Payload::SenderLayerWants(wants)) => {
                self.media
                    .set_wanted(&wants.track_id, &wants.active_layers, &self.events);
            }
            Some(Payload::Resumed(_)) => {
                let _ = self.events.send(VoiceEvent::Resumed);
            }
            _ => {}
        }
    }

    /// A refused publish, or a running track the node stopped.
    fn on_rejected(&mut self, rejected: voice::TrackRejected) {
        if let Some((_, reply)) = self.publishing.remove(&rejected.track_id) {
            let _ = reply.send(Err(TrackError::Refused {
                reason: rejected.reason,
                message: rejected.message,
            }));
            return;
        }
        self.media.unpublish(&rejected.track_id);
        let _ = self.events.send(VoiceEvent::TrackStopped {
            track_id: rejected.track_id,
            reason: rejected.reason,
            message: rejected.message,
        });
    }

    fn on_track_update(&mut self, update: voice::TrackUpdate) {
        let Some(track) = update.track else {
            return;
        };
        if track.state == voice::TrackState::Removed as i32 {
            self.media.unwatch(update.user_id, &track.track_id);
            let _ = self.events.send(VoiceEvent::TrackRemoved {
                user_id: update.user_id,
                track_id: track.track_id,
            });
            return;
        }
        self.media.watch(update.user_id, &track);
        if let Some(remote) = remote_track(&track, Some(update.available_layers)) {
            let _ = self.events.send(VoiceEvent::Track {
                user_id: update.user_id,
                track: remote,
            });
        }
    }

    /// Opens the gateway again and resumes, in the background so media
    /// keeps flowing meanwhile.
    fn resume_later(&self) -> Reconnect {
        let url = self.url.clone();
        let fingerprint = self.fingerprint;
        let resume = voice::Resume {
            voice_session_id: self.voice_session_id.clone(),
            resume_token: self.resume_token.clone(),
            last_seq: self.last_seq,
        };
        tokio::spawn(async move {
            let give_up = Instant::now() + RESUME_FOR;
            loop {
                match resume_once(&url, fingerprint, resume.clone()).await {
                    Ok(socket) => return Ok(socket),
                    Err(Some(code)) if !close::is_resumable(Some(code)) => return Err(Some(code)),
                    Err(_) if Instant::now() >= give_up => return Err(None),
                    Err(_) => tokio::time::sleep(RESUME_PAUSE).await,
                }
            }
        })
    }

    fn finish(&mut self, code: Option<u16>) {
        for (_, (_, reply)) in self.publishing.drain() {
            let _ = reply.send(Err(TrackError::NotConnected));
        }
        let _ = self.events.send(VoiceEvent::Closed { code });
    }
}

async fn resume_once(
    url: &GatewayUrl,
    fingerprint: Option<[u8; 32]>,
    resume: voice::Resume,
) -> Result<Socket, Option<u16>> {
    let mut socket = gateway::open(url, fingerprint).await.map_err(|_| None)?;
    hello(&mut socket).await.map_err(|error| match error {
        TransportError::Refused(code) => code,
        _ => None,
    })?;
    gateway::send(&mut socket, Payload::Resume(resume))
        .await
        .map_err(|_| None)?;
    Ok(socket)
}

async fn next_or_pending(socket: Option<&mut Socket>) -> Next {
    match socket {
        Some(socket) => gateway::next(socket).await,
        None => std::future::pending().await,
    }
}

async fn join_or_pending(
    handle: Option<&mut Reconnect>,
) -> Result<Result<Socket, Option<u16>>, tokio::task::JoinError> {
    match handle {
        Some(handle) => handle.await,
        None => std::future::pending().await,
    }
}

async fn sleep_until_maybe(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}
