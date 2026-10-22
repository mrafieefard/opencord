import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/voice/recording_bar.dart';
import 'package:opencord/features/chat/voice/voice_recorder_state.dart';

import '../support/pump.dart';

const _channel = (server: 'test', channel: 1);

class _StaticRecorderNotifier extends VoiceRecorderNotifier {
  _StaticRecorderNotifier(super.channel, this._initial);
  final VoiceRecorderState _initial;

  @override
  VoiceRecorderState build() => _initial;
}

void main() {
  for (final (name, colors) in themes) {
    testWidgets('recording bar states ($name)', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        surface: const Size(600, 100),
        ProviderScope(
          overrides: [
            voiceRecorderProvider(_channel).overrideWith(
              () => _StaticRecorderNotifier(
                _channel,
                const VoiceRecorderState(
                  phase: RecordingPhase.holding,
                  elapsed: Duration(seconds: 4, milliseconds: 200),
                  bars: [0.2, 0.5, 0.8, 0.3, 0.6, 0.9, 0.4, 0.7],
                ),
              ),
            ),
          ],
          child: const Padding(
            padding: EdgeInsets.all(16),
            child: RecordingBar(channel: _channel),
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/recording_bar_$name.png'),
      );
    });
  }
}
