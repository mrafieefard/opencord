import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// What to share and how well. [source] is null on Linux, where the
/// desktop's own picker (xdg-desktop-portal) chooses it.
typedef ScreenshareChoice = ({String? source, ScreenQuality quality});

typedef _Source = ({String id, String label, String? detail});

// Phase 2 lists the real screens and windows.
const _screens = <_Source>[
  (id: 'screen-1', label: 'Screen 1', detail: '2560 × 1440'),
  (id: 'screen-2', label: 'Screen 2', detail: '1920 × 1080'),
];
const _windows = <_Source>[
  (id: 'window-opencord', label: 'Opencord', detail: null),
  (id: 'window-browser', label: 'Browser', detail: null),
  (id: 'window-terminal', label: 'Terminal', detail: null),
];

/// The screenshare source picker (§4.10), with the quality options.
Future<ScreenshareChoice?> showScreensharePicker(BuildContext context) =>
    showOcDialog<ScreenshareChoice>(
      context: context,
      builder: (context) => const _ScreensharePicker(),
    );

class _ScreensharePicker extends ConsumerStatefulWidget {
  const _ScreensharePicker();

  @override
  ConsumerState<_ScreensharePicker> createState() => _ScreensharePickerState();
}

class _ScreensharePickerState extends ConsumerState<_ScreensharePicker> {
  late var _quality = ref.read(screenQualityProvider);
  var _source = _screens.first.id;

  bool get _portal => Theme.of(context).platform == TargetPlatform.linux;

  void _goLive() {
    ref.read(screenQualityProvider.notifier).set(_quality);
    Navigator.pop<ScreenshareChoice>(context, (
      source: _portal ? null : _source,
      quality: _quality,
    ));
  }

  Widget _sources(List<_Source> sources, IconData icon) =>
      SettingsChoiceCards<String>(
        options: [
          for (final source in sources)
            ChoiceCardOption(
              value: source.id,
              label: source.label,
              description: source.detail,
              icon: icon,
              preview: AspectRatio(
                aspectRatio: 16 / 9,
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(OcRadius.quote),
                  child: ColoredBox(
                    color: context.oc.rail,
                    child: const ScreenFeed(),
                  ),
                ),
              ),
            ),
        ],
        value: _source,
        onChanged: (source) => setState(() => _source = source),
      );

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return OcDialog(
      title: 'Share your screen',
      width: 600,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(label: 'Go live', onPressed: _goLive),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (_portal)
            Text(
              'Your desktop asks which screen or window to share once you '
              'go live.',
              style: OcText.body.copyWith(color: colors.textSecondary),
            )
          else ...[
            const SectionLabel('Screens'),
            const SizedBox(height: OcSpace.s6),
            _sources(_screens, OcIcons.monitor),
            const SizedBox(height: OcSpace.s16),
            const SectionLabel('Windows'),
            const SizedBox(height: OcSpace.s6),
            _sources(_windows, OcIcons.webAsset),
          ],
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Quality'),
          const SizedBox(height: OcSpace.s6),
          ChoiceChips<ScreenQuality>(
            options: [
              for (final quality in ScreenQuality.values)
                (quality, quality.label),
            ],
            value: _quality,
            onChanged: (quality) => setState(() => _quality = quality),
          ),
        ],
      ),
    );
  }
}
