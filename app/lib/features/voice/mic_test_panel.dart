import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/voice/input_level_meter.dart';
import 'package:opencord/features/voice/mic_test.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The microphone test controls (§8.3): note, meter while active, and
/// Test / Stop test button.
class MicTestPanel extends ConsumerStatefulWidget {
  const MicTestPanel({super.key});

  @override
  ConsumerState<MicTestPanel> createState() => _MicTestPanelState();
}

class _MicTestPanelState extends ConsumerState<MicTestPanel> {
  bool _startedByThis = false;
  late final MicTestNotifier _notifier;

  @override
  void initState() {
    super.initState();
    _notifier = ref.read(micTestProvider.notifier);
  }

  Future<void> _toggle() async {
    final testing = ref.read(micTestProvider);
    if (testing) {
      _startedByThis = false;
      await _notifier.stop();
    } else {
      _startedByThis = true;
      try {
        await _notifier.start();
      } on RepoException catch (e) {
        _startedByThis = false;
        if (mounted) {
          showOcToast(context, e.message);
        }
      }
    }
  }

  @override
  void dispose() {
    if (_startedByThis) {
      scheduleMicrotask(_notifier.stop);
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final inCall = ref.watch(voiceSessionProvider.select((s) => s.connected));
    final testing = ref.watch(micTestProvider);

    final note = inCall
        ? "Hear what Opencord sends. You're deafened until you stop. Use headphones, or you may hear an echo."
        : 'Hear what Opencord sends. Use headphones, or you may hear an echo.';

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          note,
          style: OcText.body.copyWith(
            fontSize: 12.5,
            color: colors.textSecondary,
          ),
        ),
        if (testing) ...[const SizedBox(height: 8), const InputLevelMeter()],
        const SizedBox(height: 8),
        OcButton(
          label: testing ? 'Stop test' : 'Test',
          variant: testing
              ? OcButtonVariant.primary
              : OcButtonVariant.secondary,
          expand: true,
          onPressed: _toggle,
        ),
      ],
    );
  }
}
