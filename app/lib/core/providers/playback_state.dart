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

  void pause(PlaybackItem item) => _whilePlaying(item, () {
    state = state.copyWith(playing: false);
    _repository.voicePause();
  });

  void resume(PlaybackItem item) => _whilePlaying(item, () {
    state = state.copyWith(playing: true);
    _repository.voiceResume();
  });

  void seek(PlaybackItem item, int positionMs) => _whilePlaying(item, () {
    state = state.copyWith(positionMs: positionMs);
    _repository.voiceSeek(positionMs);
  });

  void setSpeed(PlaybackItem item, double speed) =>
      _whilePlaying(item, () => _repository.voiceSetSpeed(speed));

  /// Stops [item] if it is the one playing.
  void stop(PlaybackItem item) => _whilePlaying(item, () {
    state = const PlaybackState();
    _repository.voiceStop();
  });

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
