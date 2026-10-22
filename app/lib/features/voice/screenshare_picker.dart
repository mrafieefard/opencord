import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/voice/screen_source_picker.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// What to share: its quality, within the server's maximum, whether its
/// sound goes too, and — where Opencord picks the source itself — which
/// screen or window.
typedef ScreenshareChoice = ({
  ScreenShareQuality quality,
  bool audio,
  String? sourceId,
});

/// The share dialog (Phase 2 plan §9.1): resolution and frame rate, with
/// options above the server's maximum disabled, and the sound switch. On
/// Linux the desktop's own picker chooses the screen or window after it.
/// [changing] asks only for a new quality, starting from [current];
/// [pickingSource] only for another screen or window, which is what
/// "Change source" does where Opencord has its own picker.
Future<ScreenshareChoice?> showScreenshareDialog(
  BuildContext context, {
  required String serverKey,
  ScreenShareQuality? current,
  bool changing = false,
  bool pickingSource = false,
}) => showOcDialog<ScreenshareChoice>(
  context: context,
  builder: (context) => _ScreenshareDialog(
    serverKey: serverKey,
    current: current,
    changing: changing,
    pickingSource: pickingSource,
  ),
);

class _ScreenshareDialog extends ConsumerStatefulWidget {
  const _ScreenshareDialog({
    required this.serverKey,
    required this.current,
    required this.changing,
    required this.pickingSource,
  });

  final String serverKey;
  final ScreenShareQuality? current;
  final bool changing;

  /// Already live, and only the source is being changed.
  final bool pickingSource;

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
  late var _sound = ref.read(shareSoundProvider);

  /// The screen or window chosen in Opencord's own picker; null until one
  /// is, which is what keeps "Go live" disabled.
  String? _source;

  /// The system's picker opens after this dialog (Linux's portal, macOS's
  /// picker); otherwise the dialog lists the sources itself.
  bool get _portal =>
      !ref.read(repositoryProvider).capabilities.ownScreenPicker;

  ScreenShareResolution get _maxResolution =>
      _settings.screenShareMaxResolution;
  int get _maxFps => _settings.screenShareMaxFps;

  bool get _capped =>
      _maxResolution != ScreenShareResolution.values.last ||
      _maxFps != ScreenShareQuality.frameRates.last;

  /// Sound goes when chosen and this system can share it.
  bool get _soundWorks =>
      ref.read(screenAudioSupportProvider).value?.supported ?? false;

  /// Whether there is enough to go live: a source, where this system makes
  /// Opencord pick one.
  bool get _ready => widget.changing || _portal || _source != null;

  void _done() {
    if (_soundWorks) ref.read(shareSoundProvider.notifier).set(_sound);
    Navigator.pop<ScreenshareChoice>(context, (
      quality: _quality,
      audio: _sound && _soundWorks,
      sourceId: _source,
    ));
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return OcDialog(
      title: switch ((widget.changing, widget.pickingSource)) {
        (true, _) => 'Change quality',
        (_, true) => 'Change source',
        _ => 'Share your screen',
      },
      width: 480,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: switch ((widget.changing, widget.pickingSource)) {
            (true, _) => 'Apply',
            (_, true) => 'Share this',
            _ => 'Go live',
          },
          onPressed: _ready ? _done : null,
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
          // Where Opencord picks, the sources come first: what to share is
          // the question, and the quality is the detail.
          if (!_portal && !widget.changing) ...[
            ScreenSourcePicker(
              chosen: _source,
              onChosen: (id) => setState(() => _source = id),
            ),
            const SizedBox(height: OcSpace.s16),
          ],
          if (!widget.pickingSource) ...[
            const SectionLabel('Resolution'),
            const SizedBox(height: OcSpace.s6),
            ChoiceChips<ScreenShareResolution>(
              options: [
                for (final resolution in ScreenShareResolution.values)
                  (resolution, resolution.label),
              ],
              value: _quality.resolution,
              isEnabled: (resolution) =>
                  resolution.index <= _maxResolution.index,
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
          ],
          if (!widget.changing && !widget.pickingSource) ...[
            const SizedBox(height: OcSpace.s16),
            switch (ref.watch(screenAudioSupportProvider)) {
              AsyncData(value: (supported: true, reason: _)) =>
                SettingsSwitchRow(
                  title: 'Share sound',
                  subtitle: _portal
                      ? 'A screen shares every app but Opencord; for a '
                            'window you choose the app.'
                      : 'Every app but Opencord.',
                  value: _sound,
                  onChanged: (on) => setState(() => _sound = on),
                ),
              AsyncData(value: (supported: false, :final reason)) =>
                SettingsSwitchRow(
                  title: 'Share sound',
                  subtitle: reason ?? "This system can't share sound yet.",
                  value: false,
                  onChanged: null,
                ),
              _ => const SettingsSwitchRow(
                title: 'Share sound',
                value: false,
                onChanged: null,
              ),
            },
          ],
        ],
      ),
    );
  }
}
