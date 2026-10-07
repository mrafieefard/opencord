import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/window/window_startup.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/settings.dart';

bool get _desktop =>
    defaultTargetPlatform == TargetPlatform.linux ||
    defaultTargetPlatform == TargetPlatform.windows ||
    defaultTargetPlatform == TargetPlatform.macOS;

/// Appearance (§8.1): theme, message density, font size, reduce motion
/// and the window frame.
class AppearancePage extends ConsumerWidget {
  const AppearancePage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final settings = ref.watch(appSettingsProvider);
    final update = ref.read(appSettingsProvider.notifier).update;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Theme',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s12),
              child: SettingsChoiceCards<ThemePreference>(
                value: settings.theme,
                onChanged: (theme) => update((s) => s.copyWith(theme: theme)),
                options: const [
                  ChoiceCardOption(
                    value: ThemePreference.system,
                    label: 'System',
                    description: 'Follows your desktop',
                    preview: ThemeMiniature.system(),
                  ),
                  ChoiceCardOption(
                    value: ThemePreference.dark,
                    label: 'Dark',
                    preview: ThemeMiniature(OcColors.dark),
                  ),
                  ChoiceCardOption(
                    value: ThemePreference.light,
                    label: 'Light',
                    preview: ThemeMiniature(OcColors.light),
                  ),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Message density',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s12),
              child: SettingsChoiceCards<MessageDensity>(
                value: settings.density,
                onChanged: (density) =>
                    update((s) => s.copyWith(density: density)),
                options: const [
                  ChoiceCardOption(
                    value: MessageDensity.comfortable,
                    label: 'Comfortable',
                    description: 'Bubbles, like Telegram',
                    preview: DensityMiniature(compact: false),
                  ),
                  ChoiceCardOption(
                    value: MessageDensity.compact,
                    label: 'Compact',
                    description: 'Lines, more on screen',
                    preview: DensityMiniature(compact: true),
                  ),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Text',
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s16,
                OcSpace.s12,
                OcSpace.s16,
                OcSpace.s4,
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      'Font size',
                      style: OcText.body.copyWith(color: colors.text),
                    ),
                  ),
                  Text(
                    '${settings.fontSize.round()} px',
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: OcSpace.s4),
              child: Slider(
                value: settings.fontSize,
                min: AppSettings.minFontSize,
                max: AppSettings.maxFontSize,
                divisions: (AppSettings.maxFontSize - AppSettings.minFontSize)
                    .round(),
                label: '${settings.fontSize.round()} px',
                semanticFormatterCallback: (value) => '${value.round()} pixels',
                onChanged: (value) =>
                    update((s) => s.copyWith(fontSize: value.roundToDouble())),
              ),
            ),
            SettingsSwitchRow(
              title: 'Reduce motion',
              subtitle: 'Turns off animations that only decorate',
              value: settings.reduceMotion,
              onChanged: (value) =>
                  update((s) => s.copyWith(reduceMotion: value)),
            ),
          ],
        ),
        if (_desktop) ...[
          const SizedBox(height: OcSpace.s24),
          SettingsSection(
            title: 'Window frame',
            footer: 'Takes effect when Opencord starts next.',
            children: [
              Padding(
                padding: const EdgeInsets.all(OcSpace.s12),
                child: SettingsChoiceCards<WindowFramePreference>(
                  value: settings.windowFrame,
                  onChanged: (frame) {
                    update((s) => s.copyWith(windowFrame: frame));
                    ref.read(frameChoiceSaverProvider)(frame);
                  },
                  options: const [
                    ChoiceCardOption(
                      value: WindowFramePreference.auto,
                      label: 'Auto',
                      description: 'Fits your desktop',
                    ),
                    ChoiceCardOption(
                      value: WindowFramePreference.custom,
                      label: 'Custom',
                      description: 'Opencord draws the title bar',
                    ),
                    ChoiceCardOption(
                      value: WindowFramePreference.system,
                      label: 'System',
                      description: 'Your desktop draws the frame',
                    ),
                  ],
                ),
              ),
            ],
          ),
        ],
      ],
    );
  }
}

/// A tiny drawing of the app in a theme (§8.1): rail, sidebar, two bubbles.
class ThemeMiniature extends StatelessWidget {
  const ThemeMiniature(this.colors, {super.key}) : system = false;

  const ThemeMiniature.system({super.key})
    : colors = OcColors.dark,
      system = true;

  final OcColors colors;

  /// Half dark, half light.
  final bool system;

  static const Size size = Size(132, 76);

  @override
  Widget build(BuildContext context) {
    final frame = BoxDecoration(
      borderRadius: BorderRadius.circular(8),
      border: Border.all(color: context.oc.border),
    );
    if (!system) {
      return Container(
        decoration: frame,
        clipBehavior: Clip.antiAlias,
        child: _drawing(colors),
      );
    }
    return Container(
      decoration: frame,
      clipBehavior: Clip.antiAlias,
      child: Stack(
        children: [
          _drawing(OcColors.light),
          ClipRect(clipper: _LeftHalf(), child: _drawing(OcColors.dark)),
        ],
      ),
    );
  }

  Widget _drawing(OcColors colors) => SizedBox.fromSize(
    size: size,
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          width: 14,
          color: colors.rail,
          padding: const EdgeInsets.symmetric(vertical: 6),
          child: Column(
            children: [
              for (var i = 0; i < 3; i++)
                Container(
                  width: 8,
                  height: 8,
                  margin: const EdgeInsets.only(bottom: 4),
                  decoration: BoxDecoration(
                    color: i == 0 ? colors.accent : colors.selected,
                    shape: BoxShape.circle,
                  ),
                ),
            ],
          ),
        ),
        Container(
          width: 34,
          color: colors.sidebar,
          padding: const EdgeInsets.fromLTRB(4, 8, 4, 4),
          child: Column(
            children: [
              for (var i = 0; i < 4; i++)
                Container(
                  height: 6,
                  margin: const EdgeInsets.only(bottom: 4),
                  decoration: BoxDecoration(
                    color: i == 1 ? colors.selected : colors.hover,
                    borderRadius: BorderRadius.circular(2),
                  ),
                ),
            ],
          ),
        ),
        Expanded(
          child: Container(
            color: colors.chat,
            padding: const EdgeInsets.all(6),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                Align(
                  alignment: Alignment.centerLeft,
                  child: Container(
                    width: 44,
                    height: 12,
                    decoration: BoxDecoration(
                      color: colors.bubbleIn,
                      borderRadius: BorderRadius.circular(4),
                      border: Border.all(color: colors.border, width: 0.5),
                    ),
                  ),
                ),
                const SizedBox(height: 4),
                Align(
                  alignment: Alignment.centerRight,
                  child: Container(
                    width: 36,
                    height: 12,
                    decoration: BoxDecoration(
                      color: colors.bubbleOut,
                      borderRadius: BorderRadius.circular(4),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    ),
  );
}

class _LeftHalf extends CustomClipper<Rect> {
  @override
  Rect getClip(Size size) => Rect.fromLTWH(0, 0, size.width / 2, size.height);

  @override
  bool shouldReclip(_LeftHalf oldClipper) => false;
}

/// Bubbles or lines, drawn small for the density choice.
class DensityMiniature extends StatelessWidget {
  const DensityMiniature({super.key, required this.compact});

  final bool compact;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    Widget bar(double width, {Color? color}) => Container(
      width: width,
      height: compact ? 5 : 10,
      decoration: BoxDecoration(
        color: color ?? colors.selected,
        borderRadius: BorderRadius.circular(compact ? 2 : 4),
      ),
    );
    return Container(
      height: ThemeMiniature.size.height,
      padding: const EdgeInsets.all(OcSpace.s8),
      decoration: BoxDecoration(
        color: colors.chat,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: colors.border),
      ),
      child: compact
          ? Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                for (final width in const [80.0, 64.0, 90.0, 52.0, 74.0])
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 2),
                    child: Row(
                      children: [
                        bar(10, color: colors.hover),
                        const SizedBox(width: 6),
                        bar(width),
                      ],
                    ),
                  ),
              ],
            )
          : Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Align(alignment: Alignment.centerLeft, child: bar(70)),
                const SizedBox(height: 6),
                Align(
                  alignment: Alignment.centerRight,
                  child: bar(54, color: colors.bubbleOut),
                ),
                const SizedBox(height: 6),
                Align(alignment: Alignment.centerLeft, child: bar(40)),
              ],
            ),
    );
  }
}
