import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';

/// A voice message or an audio file in a channel's messages: what the
/// audio player bar plays, with where to find it.
@immutable
class AudioEntry {
  const AudioEntry({
    required this.serverKey,
    required this.channelId,
    required this.messageId,
    required this.authorId,
    required this.attachment,
  });

  final String serverKey;
  final int channelId;
  final int messageId;
  final int authorId;
  final Attachment attachment;

  bool get voice => attachment.kind == AttachmentKind.voiceMessage;

  AttachmentPlayback get item => AttachmentPlayback(serverKey, attachment.id);

  @override
  bool operator ==(Object other) =>
      other is AudioEntry &&
      other.serverKey == serverKey &&
      other.attachment.id == attachment.id;

  @override
  int get hashCode => Object.hash(serverKey, attachment.id);
}

/// What plays, a voice message or media: one item at a time, app-wide.
@immutable
class PlaybackState {
  const PlaybackState({
    this.item,
    this.positionMs = 0,
    this.durationMs = 0,
    this.playing = false,
    this.buffering = false,
    this.error,
    this.speed = 1.0,
    this.origin,
  });

  final PlaybackItem? item;
  final int positionMs;
  final int durationMs;
  final bool playing;
  final bool buffering;

  /// Why the last attempt to play [item] failed.
  final String? error;

  /// A voice message's speed: 1, 1.5 or 2.
  final double speed;

  /// The message [item] is in, when it is a voice message or an audio
  /// file played from the chat: the audio player bar shows it.
  final AudioEntry? origin;

  PlaybackState copyWith({
    int? positionMs,
    bool? playing,
    bool? buffering,
    String? Function()? error,
    double? speed,
  }) => PlaybackState(
    item: item,
    positionMs: positionMs ?? this.positionMs,
    durationMs: durationMs,
    playing: playing ?? this.playing,
    buffering: buffering ?? this.buffering,
    error: error != null ? error() : this.error,
    speed: speed ?? this.speed,
    origin: origin,
  );
}

/// The voice messages and audio files of [origin]'s channel, oldest first:
/// the audio player bar's queue while [origin] plays. Just [origin] when
/// its channel's messages don't hold it.
final audioQueueProvider = Provider.family<List<AudioEntry>, AudioEntry>((
  ref,
  origin,
) {
  final messages = ref
      .watch(
        channelMessagesProvider((
          server: origin.serverKey,
          channel: origin.channelId,
        )),
      )
      .messages;
  final entries = [
    for (final message in messages)
      for (final attachment in message.attachments)
        if (attachment.id > 0 &&
            (attachment.kind == AttachmentKind.voiceMessage ||
                attachment.kind == AttachmentKind.audio))
          AudioEntry(
            serverKey: origin.serverKey,
            channelId: origin.channelId,
            messageId: message.id,
            authorId: message.authorId,
            attachment: attachment,
          ),
  ];
  return entries.contains(origin) ? entries : [origin];
});

/// Fed by the playback events `_route` passes on. Pause, resume, seek and
/// speed act only on the item playing: the core has one player.
class PlaybackNotifier extends Notifier<PlaybackState> {
  @override
  PlaybackState build() => const PlaybackState();

  OpencordRepository get _repository => ref.read(repositoryProvider);

  /// The attachment open in the media player, playing or not. The voice
  /// player can play meanwhile; the media stays open until closed.
  AttachmentPlayback? _media;

  /// The open media's message, when it is an audio file from the chat.
  AudioEntry? _mediaOrigin;

  bool _isMedia(PlaybackItem item) => item == _media;

  void apply(RepoEvent event) {
    switch (event) {
      case PlaybackUpdate():
        final same = event.item == state.item;
        state = PlaybackState(
          item: event.item,
          positionMs: event.positionMs,
          durationMs: event.durationMs,
          playing: event.playing,
          buffering: event.buffering,
          speed: same ? state.speed : 1.0,
          origin: same
              ? state.origin
              : (_isMedia(event.item) ? _mediaOrigin : null),
        );
      // The open media replays from the start on play; a voice message's
      // player ends with it, so the next play starts a new one. Either
      // way the next one in the audio player's queue starts, as on
      // Telegram.
      case PlaybackEnded(:final item) when item == state.item:
        final next = _beside(1);
        state = _isMedia(item)
            ? state.copyWith(playing: false, positionMs: 0)
            : const PlaybackState();
        if (next != null) unawaited(playEntry(next));
      case PlaybackFailed(:final item, :final message) when item == state.item:
        state = state.copyWith(
          playing: false,
          buffering: false,
          error: () => message,
        );
      default:
    }
  }

  Future<void> play(
    PlaybackItem item, {
    int positionMs = 0,
    double speed = 1.0,
    AudioEntry? origin,
  }) async {
    state = PlaybackState(
      item: item,
      positionMs: positionMs,
      playing: true,
      buffering: true,
      speed: speed,
      origin: origin,
    );
    try {
      await _repository.voicePlay(item, positionMs, speed);
    } on RepoException catch (error) {
      if (state.item == item) {
        state = state.copyWith(
          playing: false,
          buffering: false,
          error: () => error.message,
        );
      }
    }
  }

  Future<MediaHandle> mediaOpen(
    AttachmentPlayback item, {
    AudioEntry? origin,
  }) async {
    _media = item;
    _mediaOrigin = origin;
    state = PlaybackState(item: item, buffering: true, origin: origin);
    try {
      final handle = await _repository.mediaOpen(
        item.serverKey,
        item.attachmentId,
      );
      state = state.copyWith(buffering: false, positionMs: 0);
      return handle;
    } on RepoException catch (error) {
      if (state.item == item) {
        state = state.copyWith(
          playing: false,
          buffering: false,
          error: () => error.message,
        );
      }
      rethrow;
    }
  }

  /// The attachment open in the media player, if any.
  AttachmentPlayback? get openMedia => _media;

  /// Plays the open media [item], taking over from a voice message if one
  /// plays; after the end it starts again.
  void mediaPlay(AttachmentPlayback item) {
    if (item != _media) return;
    state = state.item == item
        ? state.copyWith(playing: true, error: () => null)
        : PlaybackState(item: item, playing: true, origin: _mediaOrigin);
    try {
      _repository.mediaPlay();
    } on RepoException catch (error) {
      state = state.copyWith(playing: false, error: () => error.message);
    }
  }

  void pause(PlaybackItem item) => _whilePlaying(item, () {
    state = state.copyWith(playing: false);
    if (_isMedia(item)) {
      _repository.mediaPause();
    } else {
      _repository.voicePause();
    }
  });

  void resume(PlaybackItem item) {
    if (item case final AttachmentPlayback media when _isMedia(media)) {
      mediaPlay(media);
      return;
    }
    _whilePlaying(item, () {
      state = state.copyWith(playing: true);
      _repository.voiceResume();
    });
  }

  void seek(PlaybackItem item, int positionMs) => _whilePlaying(item, () {
    state = state.copyWith(positionMs: positionMs);
    if (_isMedia(item)) {
      _repository.mediaSeek(positionMs);
    } else {
      _repository.voiceSeek(positionMs);
    }
  });

  void setSpeed(PlaybackItem item, double speed) => _whilePlaying(item, () {
    state = state.copyWith(speed: speed);
    _repository.voiceSetSpeed(speed);
  });

  /// Plays [entry] from the audio player bar: a voice message in the voice
  /// player, an audio file in the media player.
  Future<void> playEntry(AudioEntry entry) async {
    if (entry.voice) {
      await play(entry.item, speed: state.speed, origin: entry);
      return;
    }
    try {
      await mediaOpen(entry.item, origin: entry);
      mediaPlay(entry.item);
    } on RepoException {
      // mediaOpen put the message in the playback state.
    }
  }

  /// The entry [step] places from the one playing in its queue, if any.
  AudioEntry? _beside(int step) {
    final origin = state.origin;
    if (origin == null) return null;
    final queue = ref.read(audioQueueProvider(origin));
    final at = queue.indexOf(origin);
    final next = at + step;
    return at < 0 || next < 0 || next >= queue.length ? null : queue[next];
  }

  AudioEntry? get previousEntry => _beside(-1);

  AudioEntry? get nextEntry => _beside(1);

  /// The audio player bar's ×: whatever it shows stops.
  void closePlayer() {
    final item = state.item;
    if (state.origin == null || item == null) return;
    if (_isMedia(item)) {
      mediaClose();
    } else {
      stop(item);
    }
  }

  void mediaSetVolume(double volume, bool muted) {
    try {
      _repository.mediaSetVolume(volume, muted);
    } on RepoException catch (error) {
      state = state.copyWith(error: () => error.message);
    }
  }

  /// Stops [item] if it is the one playing.
  void stop(PlaybackItem item) => _whilePlaying(item, () {
    if (_isMedia(item)) {
      mediaClose();
    } else {
      state = const PlaybackState();
      _repository.voiceStop();
    }
  });

  /// Closes the open media, whether or not it is what plays now.
  void mediaClose() => _closeMedia(stateLater: false);

  /// [mediaClose] from a widget's `dispose`. The player stops at once; the
  /// state follows after the frame, as providers can't change while the
  /// widget tree is being finalized.
  void mediaCloseOnDispose() => _closeMedia(stateLater: true);

  void _closeMedia({required bool stateLater}) {
    final media = _media;
    if (media == null) return;
    _media = null;
    _mediaOrigin = null;
    String? failure;
    try {
      _repository.mediaClose();
    } on RepoException catch (error) {
      failure = error.message;
    }
    void settle() {
      if (state.item == media) state = const PlaybackState();
      if (failure != null) state = state.copyWith(error: () => failure);
    }

    if (stateLater) {
      Future.microtask(() {
        if (ref.mounted) settle();
      });
    } else {
      settle();
    }
  }

  void _whilePlaying(PlaybackItem item, void Function() action) {
    if (state.item != item) return;
    try {
      action();
    } on RepoException catch (error) {
      state = state.copyWith(playing: false, error: () => error.message);
    }
  }
}

final playbackProvider = NotifierProvider<PlaybackNotifier, PlaybackState>(
  PlaybackNotifier.new,
);
