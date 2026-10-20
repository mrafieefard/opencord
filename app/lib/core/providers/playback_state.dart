import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';

/// What the voice player is playing: one item at a time, app-wide.
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
  bool _isMedia = false;

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
      case PlaybackEnded(:final item) when item == state.item:
        state = state.copyWith(playing: false, positionMs: 0);
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
    _isMedia = false;
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
    _isMedia = true;
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

  void mediaPlay(AttachmentPlayback item) => _whilePlaying(item, () {
    state = state.copyWith(playing: true);
    _repository.mediaPlay();
  });

  void pause(PlaybackItem item) => _whilePlaying(item, () {
    state = state.copyWith(playing: false);
    if (_isMedia) {
      _repository.mediaPause();
    } else {
      _repository.voicePause();
    }
  });

  void resume(PlaybackItem item) => _whilePlaying(item, () {
    state = state.copyWith(playing: true);
    if (_isMedia) {
      _repository.mediaPlay();
    } else {
      _repository.voiceResume();
    }
  });

  void seek(PlaybackItem item, int positionMs) => _whilePlaying(item, () {
    state = state.copyWith(positionMs: positionMs);
    if (_isMedia) {
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
    state = const PlaybackState();
    if (_isMedia) {
      _isMedia = false;
      _repository.mediaClose();
    } else {
      _repository.voiceStop();
    }
  });

  void mediaClose() {
    if (_isMedia) {
      _isMedia = false;
      state = const PlaybackState();
      try {
        _repository.mediaClose();
      } on RepoException catch (error) {
        state = state.copyWith(error: () => error.message);
      }
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
