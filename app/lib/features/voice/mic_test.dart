import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/src/rust/api/logs.dart' show LogLevel;

/// Whether the microphone test is on: hearing what Opencord sends (spec
/// §8.4). Never saved; it always starts off.
final micTestProvider = NotifierProvider<MicTestNotifier, bool>(
  MicTestNotifier.new,
);

/// Starts and stops the test. In a call, starting deafens this person, so
/// nobody hears the test; stopping undeafens them only if starting did.
/// Leaving or joining a call, or undeafening any other way, ends it.
class MicTestNotifier extends Notifier<bool> {
  /// What was asked last; the work catches up with it.
  var _wanted = false;

  /// Whether the core has the test on.
  var _on = false;

  /// Whether starting the test deafened this person.
  var _deafenedIt = false;

  /// While this notifier deafens or undeafens: not someone else's doing.
  var _ownChange = false;

  /// The work under way, which every caller until it ends waits for.
  Future<void>? _settling;

  @override
  bool build() {
    ref.listen(voiceSessionProvider, _voiceChanged);
    return false;
  }

  OpencordRepository get _repository => ref.read(repositoryProvider);

  /// Starts the test; throws [RepoException], having put everything back.
  Future<void> start() => _ask(true);

  /// Ends the test; safe to call when it is off.
  Future<void> stop() => _ask(false);

  /// Quick calls end on the last one asked.
  Future<void> _ask(bool on) {
    _wanted = on;
    state = on;
    if (_settling case final running?) return running;
    if (_on == on) return Future.value();
    return _settling = _settle();
  }

  Future<void> _settle() async {
    try {
      while (_on != _wanted) {
        if (_wanted) {
          await _begin();
        } else {
          await _end();
        }
      }
    } finally {
      _settling = null;
    }
  }

  Future<void> _begin() async {
    final voice = ref.read(voiceSessionProvider);
    _deafenedIt = voice.connected && !voice.deafened;
    try {
      // Deafened first, so the core never tests in a call that sends.
      if (_deafenedIt) await _toggleDeafen();
      await _repository.setMicTest(true);
      _on = true;
    } on RepoException {
      _wanted = false;
      state = false;
      await _undeafen();
      rethrow;
    }
  }

  Future<void> _end() async {
    await _repository.setMicTest(false);
    _on = false;
    await _undeafen();
  }

  /// Undeafens if starting the test deafened this person and they still
  /// are.
  Future<void> _undeafen() async {
    final deafenedIt = _deafenedIt;
    _deafenedIt = false;
    if (deafenedIt && ref.read(voiceSessionProvider).deafened) {
      await _toggleDeafen();
    }
  }

  Future<void> _toggleDeafen() async {
    _ownChange = true;
    try {
      await ref.read(voiceSessionProvider.notifier).toggleDeafen();
    } finally {
      _ownChange = false;
    }
  }

  void _voiceChanged(VoiceSession? previous, VoiceSession next) {
    if (!_wanted || _ownChange || previous == null) return;
    final moved =
        previous.serverKey != next.serverKey ||
        previous.channelId != next.channelId;
    final undeafened = previous.deafened && !next.deafened;
    if (moved || undeafened) unawaited(_stopForVoice());
  }

  Future<void> _stopForVoice() async {
    try {
      await stop();
    } on RepoException catch (error) {
      // Nothing on screen asked for this stop, and the voice state that
      // ended the test is already shown; the log keeps why it failed.
      appLog(
        'the microphone test did not stop',
        name: 'voice',
        error: error,
        level: LogLevel.warn,
      );
    }
  }
}
