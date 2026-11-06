import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/voice/mic_test_panel.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_segmented.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// The noise suppression chip in the voice card (§8.1).
class NoiseChip extends ConsumerStatefulWidget {
  const NoiseChip({super.key});

  @override
  ConsumerState<NoiseChip> createState() => _NoiseChipState();
}

class _NoiseChipState extends ConsumerState<NoiseChip> {
  final _chipKey = GlobalKey();
  bool _popoverOpen = false;

  Future<void> _openPopover() async {
    final context = _chipKey.currentContext;
    if (context == null) return;
    setState(() => _popoverOpen = true);
    try {
      await showPopover<void>(
        context: context,
        anchor: globalRectOf(context),
        side: PopoverSide.right,
        builder: (_) => const NoiseSuppressionPopover(),
      );
    } finally {
      if (mounted) setState(() => _popoverOpen = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final mode = ref.watch(
      audioSettingsProvider.select(
        (a) => a.noiseSuppression ?? NoiseSuppression.standard,
      ),
    );

    final label = switch (mode) {
      NoiseSuppression.high => 'High',
      NoiseSuppression.standard => 'Standard',
      NoiseSuppression.off => 'Off',
    };

    final rounded = BorderRadius.circular(12);

    return Tooltip(
      message: 'Noise suppression: $label',
      excludeFromSemantics: true,
      child: Semantics(
        label: 'Noise suppression: $label',
        button: true,
        child: Hoverable(
          key: _chipKey,
          onTap: _openPopover,
          focusRadius: rounded,
          builder: (context, state) {
            final active = state.active || _popoverOpen;
            final bg = active ? colors.selected : colors.hover;
            final fg = active
                ? colors.text
                : (mode == NoiseSuppression.off
                      ? colors.textMuted
                      : colors.textSecondary);

            return AnimatedContainer(
              duration: OcMotion.of(context).hover,
              width: 24,
              height: 24,
              decoration: BoxDecoration(color: bg, borderRadius: rounded),
              alignment: Alignment.center,
              child: Icon(OcIcons.graphicEq, size: 16, color: fg),
            );
          },
        ),
      ),
    );
  }
}

/// The popover opened from [NoiseChip] (§8.2): mode toggle, switch,
/// description, and [MicTestPanel].
class NoiseSuppressionPopover extends ConsumerStatefulWidget {
  const NoiseSuppressionPopover({super.key});

  static const width = 264.0;

  @override
  ConsumerState<NoiseSuppressionPopover> createState() =>
      _NoiseSuppressionPopoverState();
}

class _NoiseSuppressionPopoverState
    extends ConsumerState<NoiseSuppressionPopover> {
  NoiseSuppression? _lastActiveMode;

  @override
  void initState() {
    super.initState();
    final current = ref.read(audioSettingsProvider).noiseSuppression;
    if (current != null && current != NoiseSuppression.off) {
      _lastActiveMode = current;
    }
  }

  void _setMode(NoiseSuppression mode) {
    if (mode != NoiseSuppression.off) {
      _lastActiveMode = mode;
    }
    ref
        .read(audioSettingsProvider.notifier)
        .update((audio) => audio.copyWith(noiseSuppression: mode));
  }

  void _onSwitchChanged(bool on) {
    if (on) {
      final target = _lastActiveMode ?? NoiseSuppression.standard;
      _setMode(target);
    } else {
      _setMode(NoiseSuppression.off);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final mode = ref.watch(
      audioSettingsProvider.select(
        (a) => a.noiseSuppression ?? NoiseSuppression.standard,
      ),
    );

    final isOff = mode == NoiseSuppression.off;

    final description = switch (mode) {
      NoiseSuppression.standard => 'Removes steady noise; very light',
      NoiseSuppression.high => 'Also keyboards and dogs; heavier',
      NoiseSuppression.off => 'Sends the microphone as it is',
    };

    return SizedBox(
      width: NoiseSuppressionPopover.width,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(
                'Noise suppression',
                style: OcText.header.copyWith(
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                  color: colors.text,
                ),
              ),
              OcSwitch(value: !isOff, onChanged: _onSwitchChanged),
            ],
          ),
          const SizedBox(height: 8),
          // Shrink-wrapped, as the People and Music switch is.
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: OcSegmented<NoiseSuppression>(
              value: isOff ? null : mode,
              options: const [
                (NoiseSuppression.standard, 'Standard'),
                (NoiseSuppression.high, 'High'),
              ],
              onChanged: _setMode,
            ),
          ),
          const SizedBox(height: 6),
          Text(
            description,
            style: OcText.body.copyWith(
              fontSize: 12.5,
              color: colors.textSecondary,
            ),
          ),
          const SizedBox(height: 12),
          Container(height: 1, color: colors.border),
          const SizedBox(height: 12),
          const MicTestPanel(),
        ],
      ),
    );
  }
}
