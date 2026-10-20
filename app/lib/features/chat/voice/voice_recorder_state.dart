import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/navigation.dart';

enum RecordingPhase { idle, holding, cancelling, locked, preview }

/// Why recording or sending a voice message failed, for a toast. Each one is
/// a new object, so the same message twice is still two toasts.
class VoiceRecorderProblem {
  VoiceRecorderProblem(this.message);

  final String message;
}

@immutable
class VoiceRecorderState {
  const VoiceRecorderState({
    this.phase = RecordingPhase.idle,
    this.elapsed = Duration.zero,
    this.bars = const [],
    this.draft,
    this.problem,
  });

  final RecordingPhase phase;
  final Duration elapsed;

  /// One level per 100 ms, 0–1.
  final List<double> bars;
  final VoiceDraft? draft;
  final VoiceRecorderProblem? problem;

  VoiceRecorderState copyWith({
    RecordingPhase? phase,
    Duration? elapsed,
    List<double>? bars,
    VoiceDraft? Function()? draft,
  }) => VoiceRecorderState(
    phase: phase ?? this.phase,
    elapsed: elapsed ?? this.elapsed,
    bars: bars ?? this.bars,
    draft: draft != null ? draft() : this.draft,
  );
}

class VoiceRecorderNotifier extends Notifier<VoiceRecorderState> {
  VoiceRecorderNotifier(this.channel);

  final ChannelRef channel;

  Timer? _timer;
  StreamSubscription<RepoEvent>? _events;

  /// The loudest level heard in the current 100 ms bar.
  double _loudest = 0;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  @override
  VoiceRecorderState build() {
    ref.onDispose(_stopListening);
    return const VoiceRecorderState();
  }

  bool get _recording => switch (state.phase) {
    RecordingPhase.holding ||
    RecordingPhase.cancelling ||
    RecordingPhase.locked => true,
    RecordingPhase.idle || RecordingPhase.preview => false,
  };

  void press() {
    _stopListening();
    final settings = ref.read(serverProvider(channel.server)).data;
    final maxSeconds =
        settings?.uploadSettings.voiceMessageMaxSeconds ??
        UploadSettings.fallback.voiceMessageMaxSeconds;
    try {
      _repository.voiceRecordStart(maxSeconds);
    } on RepoException catch (error) {
      _fail(error.message);
      return;
    }
    _loudest = 0;
    state = const VoiceRecorderState(phase: RecordingPhase.holding);
    _events = _repository.events.listen(_onEvent);
    _timer = Timer.periodic(const Duration(milliseconds: 100), _onTick);
  }

  void _onTick(Timer timer) {
    if (!_recording) return;
    state = state.copyWith(
      elapsed: state.elapsed + const Duration(milliseconds: 100),
      bars: [...state.bars, _loudest],
    );
    _loudest = 0;
  }

  void _onEvent(RepoEvent event) {
    switch (event) {
      case VoiceRecordLevel(:final level):
        _loudest = math.max(_loudest, level);
      // At the 30 minutes the recording stops, unsent: the member decides
      // in the preview, whether they still hold the button or locked it.
      case VoiceRecordStopped(reason: VoiceStopReason.maxLength):
        if (state.phase == RecordingPhase.holding ||
            state.phase == RecordingPhase.locked) {
          unawaited(stopForPreview());
        }
      case VoiceRecordStopped(reason: VoiceStopReason.deviceLost):
        // Keep what was heard: the member decides whether to send it.
        unawaited(stopForPreview(lost: true));
      default:
    }
  }

  void move(Offset delta) {
    if (state.phase == RecordingPhase.holding) {
      if (delta.dy < -60) {
        state = state.copyWith(phase: RecordingPhase.locked);
      } else if (delta.dx < -120) {
        state = state.copyWith(phase: RecordingPhase.cancelling);
      }
    } else if (state.phase == RecordingPhase.cancelling) {
      if (delta.dy < -60) {
        state = state.copyWith(phase: RecordingPhase.locked);
      } else if (delta.dx > -120) {
        state = state.copyWith(phase: RecordingPhase.holding);
      }
    }
  }

  /// False when the press was too short to record (it is cancelled).
  bool release() {
    if (state.phase == RecordingPhase.holding) {
      if (state.elapsed < const Duration(milliseconds: 500)) {
        cancel();
        return false;
      }
      unawaited(send());
    } else if (state.phase == RecordingPhase.cancelling) {
      cancel();
    }
    return true;
  }

  void cancel() {
    _stopListening();
    try {
      _repository.voiceRecordCancel();
    } on RepoException catch (error) {
      _fail(error.message);
      return;
    }
    state = const VoiceRecorderState();
  }

  void lock() {
    if (state.phase == RecordingPhase.holding ||
        state.phase == RecordingPhase.cancelling) {
      state = state.copyWith(phase: RecordingPhase.locked);
    }
  }

  Future<void> stopForPreview({bool lost = false}) async {
    if (!_recording) return;
    _stopListening();
    try {
      final draft = await _repository.voiceRecordStop();
      state = VoiceRecorderState(
        phase: RecordingPhase.preview,
        elapsed: state.elapsed,
        bars: state.bars,
        draft: draft,
        problem: lost
            ? VoiceRecorderProblem(
                'The microphone stopped. What it recorded is kept.',
              )
            : null,
      );
    } on RepoException catch (error) {
      _fail(error.message);
    }
  }

  Future<void> send() async {
    _stopListening();
    final VoiceDraft draft;
    if (state.draft case final kept?) {
      draft = kept;
      ref.read(playbackProvider.notifier).stop(DraftPlayback(kept.id));
    } else {
      try {
        draft = await _repository.voiceRecordStop();
      } on RepoException catch (error) {
        _fail(error.message);
        return;
      }
    }
    state = const VoiceRecorderState();
    await ref.read(channelMessagesProvider(channel).notifier).sendVoice(draft);
  }

  void discard() {
    _stopListening();
    try {
      if (state.draft case final draft?) {
        ref.read(playbackProvider.notifier).stop(DraftPlayback(draft.id));
        _repository.voiceDraftDiscard(draft.id);
      } else {
        _repository.voiceRecordCancel();
      }
    } on RepoException catch (error) {
      _fail(error.message);
      return;
    }
    state = const VoiceRecorderState();
  }

  void _fail(String message) {
    _stopListening();
    state = VoiceRecorderState(problem: VoiceRecorderProblem(message));
  }

  void _stopListening() {
    _timer?.cancel();
    _timer = null;
    unawaited(_events?.cancel());
    _events = null;
  }
}

final voiceRecorderProvider =
    NotifierProvider.family<
      VoiceRecorderNotifier,
      VoiceRecorderState,
      ChannelRef
    >(VoiceRecorderNotifier.new);

/// True if currently recording a voice message in any active channel.
final isRecordingVoiceProvider = Provider<bool>((ref) {
  final current = ref.watch(currentChannelRefProvider);
  if (current == null) return false;
  final phase = ref.watch(
    voiceRecorderProvider(current).select((s) => s.phase),
  );
  return phase == RecordingPhase.holding ||
      phase == RecordingPhase.cancelling ||
      phase == RecordingPhase.locked;
});
