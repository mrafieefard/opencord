import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';

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
  });

  final PlaybackItem? item;
  final int positionMs;
  final int durationMs;
  final bool playing;
  final bool buffering;

  /// Why the last attempt to play [item] failed.
  final String? error;

  PlaybackState copyWith({
    int? positionMs,
    bool? playing,
    bool? buffering,
    String? Function()? error,
  }) => PlaybackState(
    item: item,
    positionMs: positionMs ?? this.positionMs,
    durationMs: durationMs,
    playing: playing ?? this.playing,
    buffering: buffering ?? this.buffering,
    error: error != null ? error() : this.error,
  );
}

/// Fed by the playback events `_route` passes on. Pause, resume, seek and
/// speed act only on the item playing: the core has one player.
class PlaybackNotifier extends Notifier<PlaybackState> {
  @override
  PlaybackState build() => const PlaybackState();

  OpencordRepository get _repository => ref.read(repositoryProvider);

  /// The attachment open in the media player, playing or not. The voice
  /// player can play meanwhile; the media stays open until closed.
  AttachmentPlayback? _media;

  bool _isMedia(PlaybackItem item) => item == _media;

  void apply(RepoEvent event) {
    switch (event) {
      case PlaybackUpdate():
        state = PlaybackState(
          item: event.item,
          positionMs: event.positionMs,
          durationMs: event.durationMs,
          playing: event.playing,
          buffering: event.buffering,
        );
      // The open media replays from the start on play; a voice message's
      // player ends with it, so the next play starts a new one.
      case PlaybackEnded(:final item) when item == state.item:
        state = _isMedia(item)
            ? state.copyWith(playing: false, positionMs: 0)
            : const PlaybackState();
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
  }) async {
    state = PlaybackState(
      item: item,
      positionMs: positionMs,
      playing: true,
      buffering: true,
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

  Future<MediaHandle> mediaOpen(AttachmentPlayback item) async {
    _media = item;
    state = PlaybackState(item: item, buffering: true);
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
        : PlaybackState(item: item, playing: true);
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

  void setSpeed(PlaybackItem item, double speed) =>
      _whilePlaying(item, () => _repository.voiceSetSpeed(speed));

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
