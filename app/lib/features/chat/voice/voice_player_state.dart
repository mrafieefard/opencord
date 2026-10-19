import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/src/rust/api/types.dart';

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
  final String? error;

  bool isItem(PlaybackItem other) {
    final cur = item;
    if (cur == null) return false;
    if (cur is PlaybackItem_Attachment && other is PlaybackItem_Attachment) {
      return cur.serverKey == other.serverKey &&
          cur.attachmentId == other.attachmentId;
    }
    if (cur is PlaybackItem_Draft && other is PlaybackItem_Draft) {
      return cur.draftId == other.draftId;
    }
    return false;
  }

  PlaybackState copyWith({
    PlaybackItem? Function()? item,
    int? positionMs,
    int? durationMs,
    bool? playing,
    bool? buffering,
    String? Function()? error,
  }) => PlaybackState(
    item: item != null ? item() : this.item,
    positionMs: positionMs ?? this.positionMs,
    durationMs: durationMs ?? this.durationMs,
    playing: playing ?? this.playing,
    buffering: buffering ?? this.buffering,
    error: error != null ? error() : this.error,
  );
}

class PlaybackNotifier extends Notifier<PlaybackState> {
  @override
  PlaybackState build() {
    final changes = ref.watch(repositoryProvider).events.listen(apply);
    ref.onDispose(changes.cancel);
    return const PlaybackState();
  }

  OpencordRepository get _repo => ref.read(repositoryProvider);

  void apply(RepoEvent event) {
    if (event is PlaybackUpdate) {
      state = PlaybackState(
        item: event.item,
        positionMs: event.positionMs,
        durationMs: event.durationMs,
        playing: event.playing,
        buffering: event.buffering,
      );
    } else if (event is PlaybackEnded) {
      if (state.isItem(event.item)) {
        state = state.copyWith(playing: false, positionMs: 0);
      }
    } else if (event is PlaybackFailed) {
      if (state.isItem(event.item)) {
        state = state.copyWith(playing: false, error: () => event.message);
      }
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
      await _repo.voicePlay(item, positionMs, speed);
    } on RepoException catch (e) {
      state = state.copyWith(
        playing: false,
        buffering: false,
        error: () => e.message,
      );
    }
  }

  void pause(PlaybackItem item) {
    state = state.copyWith(playing: false);
    _repo.voicePause(item);
  }

  void resume(PlaybackItem item) {
    state = state.copyWith(playing: true);
    _repo.voiceResume(item);
  }

  void seek(PlaybackItem item, int positionMs) {
    state = state.copyWith(positionMs: positionMs);
    _repo.voiceSeek(item, positionMs);
  }

  void setSpeed(PlaybackItem item, double speed) {
    _repo.voiceSetSpeed(item, speed);
  }

  void stop() {
    state = const PlaybackState();
    _repo.voiceStop();
  }
}

final playbackProvider = NotifierProvider<PlaybackNotifier, PlaybackState>(
  PlaybackNotifier.new,
);
