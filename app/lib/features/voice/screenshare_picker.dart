import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// What to share: its quality, within the server's maximum, and whether
/// its sound goes too.
typedef ScreenshareChoice = ({ScreenShareQuality quality, bool audio});

/// The share dialog (Phase 2 plan §9.1): resolution and frame rate, with
/// options above the server's maximum disabled, and the sound switch. On
/// Linux the desktop's own picker chooses the screen or window after it.
/// [changing] asks only for a new quality, starting from [current].
Future<ScreenshareChoice?> showScreenshareDialog(
  BuildContext context, {
  required String serverKey,
  ScreenShareQuality? current,
  bool changing = false,
}) => showOcDialog<ScreenshareChoice>(
  context: context,
  builder: (context) => _ScreenshareDialog(
    serverKey: serverKey,
    current: current,
    changing: changing,
  ),
);

class _ScreenshareDialog extends ConsumerStatefulWidget {
  const _ScreenshareDialog({
    required this.serverKey,
    required this.current,
    required this.changing,
  });

  final String serverKey;
  final ScreenShareQuality? current;
  final bool changing;

  @override
  ConsumerState<_ScreenshareDialog> createState() => _ScreenshareDialogState();
}

class _ScreenshareDialogState extends ConsumerState<_ScreenshareDialog> {
  late final VoiceSettings _settings =
      ref.read(serverProvider(widget.serverKey)).data?.voiceSettings ??
      const VoiceSettings();
  late var _quality = startingQuality(
    _settings,
    widget.current ?? ref.read(lastScreenQualityProvider(widget.serverKey)),
  );

  bool get _portal => Theme.of(context).platform == TargetPlatform.linux;

  ScreenShareResolution get _maxResolution =>
      _settings.screenShareMaxResolution;
  int get _maxFps => _settings.screenShareMaxFps;

  bool get _capped =>
      _maxResolution != ScreenShareResolution.values.last ||
      _maxFps != ScreenShareQuality.frameRates.last;

  void _done() => Navigator.pop<ScreenshareChoice>(context, (
    quality: _quality,
    // Sharing sound arrives with V7.
    audio: false,
  ));

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return OcDialog(
      title: widget.changing ? 'Change quality' : 'Share your screen',
      width: 480,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: widget.changing ? 'Apply' : 'Go live',
          onPressed: _done,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (_portal && !widget.changing) ...[
            Text(
              'Your desktop asks which screen or window to share once you '
              'go live.',
              style: OcText.body.copyWith(color: colors.textSecondary),
            ),
            const SizedBox(height: OcSpace.s16),
          ],
          const SectionLabel('Resolution'),
          const SizedBox(height: OcSpace.s6),
          ChoiceChips<ScreenShareResolution>(
            options: [
              for (final resolution in ScreenShareResolution.values)
                (resolution, resolution.label),
            ],
            value: _quality.resolution,
            isEnabled: (resolution) => resolution.index <= _maxResolution.index,
            onChanged: (resolution) => setState(
              () => _quality = ScreenShareQuality(resolution, _quality.fps),
            ),
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Frame rate'),
          const SizedBox(height: OcSpace.s6),
          ChoiceChips<int>(
            options: [
              for (final fps in ScreenShareQuality.frameRates)
                (fps, '$fps fps'),
            ],
            value: _quality.fps,
            isEnabled: (fps) => fps <= _maxFps,
            onChanged: (fps) => setState(
              () => _quality = ScreenShareQuality(_quality.resolution, fps),
            ),
          ),
          if (_capped) ...[
            const SizedBox(height: OcSpace.s8),
            Text(
              'Server limit: ${_maxResolution.label} · $_maxFps fps',
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ],
          if (!widget.changing) ...[
            const SizedBox(height: OcSpace.s16),
            const SettingsSwitchRow(
              title: 'Share computer sound (except Opencord)',
              subtitle: 'Sharing sound comes in a later update.',
              value: false,
              onChanged: null,
            ),
          ],
        ],
      ),
    );
  }
}
