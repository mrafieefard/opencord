import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/navigation.dart';

enum RecordingPhase { idle, holding, cancelling, locked, preview }

@immutable
class VoiceRecorderState {
  const VoiceRecorderState({
    this.phase = RecordingPhase.idle,
    this.elapsed = Duration.zero,
    this.bars = const [],
    this.draft,
  });

  final RecordingPhase phase;
  final Duration elapsed;
  final List<double> bars;
  final VoiceDraft? draft;

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

double _rmsToLevel(double rms) {
  final safe = math.max(rms, 1e-9);
  final db = 20 * (math.log(safe) / math.ln10);
  return ((db + 60) / 60).clamp(0.0, 1.0);
}

class VoiceRecorderNotifier extends Notifier<VoiceRecorderState> {
  VoiceRecorderNotifier(this.channel);

  final ChannelRef channel;

  Timer? _timer;
  StreamSubscription<RepoEvent>? _eventsSub;
  double _currentWindowMaxRms = 0;

  OpencordRepository get _repo => ref.read(repositoryProvider);

  @override
  VoiceRecorderState build() {
    ref.onDispose(() {
      _timer?.cancel();
      _eventsSub?.cancel();
    });
    return const VoiceRecorderState();
  }

  void press() {
    _cleanup();
    final serverData = ref.read(serverProvider(channel.server)).data;
    final maxSeconds = serverData?.uploadSettings.voiceMessageMaxSeconds ?? 300;

    _repo.voiceRecordStart(maxSeconds);
    _currentWindowMaxRms = 0;

    state = const VoiceRecorderState(
      phase: RecordingPhase.holding,
      elapsed: Duration.zero,
      bars: [],
    );

    _eventsSub = _repo.events.listen(_onRepoEvent);
    _timer = Timer.periodic(const Duration(milliseconds: 100), _onTick);
  }

  void _onTick(Timer timer) {
    if (state.phase != RecordingPhase.holding &&
        state.phase != RecordingPhase.cancelling &&
        state.phase != RecordingPhase.locked) {
      return;
    }
    final elapsed = state.elapsed + const Duration(milliseconds: 100);

    final barLevel = _rmsToLevel(_currentWindowMaxRms);
    _currentWindowMaxRms = 0;

    final updatedBars = List<double>.from(state.bars)..add(barLevel);
    state = state.copyWith(elapsed: elapsed, bars: updatedBars);
  }

  void _onRepoEvent(RepoEvent event) {
    if (event is VoiceRecordLevel) {
      if (event.rms > _currentWindowMaxRms) {
        _currentWindowMaxRms = event.rms;
      }
    } else if (event is VoiceRecordStopped) {
      if (event.reason == VoiceStopReason.maxLength) {
        if (state.phase == RecordingPhase.holding) {
          send();
        } else if (state.phase == RecordingPhase.locked) {
          stopForPreview();
        }
      } else if (event.reason == VoiceStopReason.deviceLost) {
        cancel();
      }
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

  /// Returns false if the press was shorter than 500 ms (cancelled and tooltip shown).
  bool release() {
    if (state.phase == RecordingPhase.holding) {
      if (state.elapsed < const Duration(milliseconds: 500)) {
        cancel();
        return false;
      }
      send();
      return true;
    } else if (state.phase == RecordingPhase.cancelling) {
      cancel();
      return true;
    }
    return true;
  }

  void cancel() {
    _cleanup();
    _repo.voiceRecordCancel();
    state = const VoiceRecorderState(phase: RecordingPhase.idle);
  }

  void lock() {
    if (state.phase == RecordingPhase.holding ||
        state.phase == RecordingPhase.cancelling) {
      state = state.copyWith(phase: RecordingPhase.locked);
    }
  }

  Future<void> stopForPreview() async {
    _timer?.cancel();
    _eventsSub?.cancel();
    try {
      final draft = await _repo.voiceRecordStop();
      state = state.copyWith(phase: RecordingPhase.preview, draft: () => draft);
    } on RepoException {
      cancel();
    }
  }

  Future<void> send([VoiceDraft? draft]) async {
    _cleanup();
    final d = draft ?? state.draft;
    final toSend = d ?? await _repo.voiceRecordStop();
    state = const VoiceRecorderState(phase: RecordingPhase.idle);
    await ref.read(channelMessagesProvider(channel).notifier).sendVoice(toSend);
  }

  void discard() {
    _cleanup();
    if (state.draft case final draft?) {
      _repo.voiceDraftDiscard(draft.draftId.toInt());
    } else {
      _repo.voiceRecordCancel();
    }
    state = const VoiceRecorderState(phase: RecordingPhase.idle);
  }

  void _cleanup() {
    _timer?.cancel();
    _timer = null;
    _eventsSub?.cancel();
    _eventsSub = null;
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
