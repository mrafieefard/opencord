import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/voice_cues.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// User settings → Sounds (Phase 2 plan §11.7, §12): how loud soundboards
/// are on this device, or not at all, and which voice sounds play.
class SoundsPage extends ConsumerWidget {
  const SoundsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final prefs = ref.watch(soundboardPrefsProvider);
    final notifier = ref.read(soundboardPrefsProvider.notifier);
    final cuesOff = ref.watch(cuePrefsProvider);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Soundboard',
          footer:
              'Sounds others play in your voice channel. Previews play at this '
              'volume too.',
          children: [
            SettingsSliderRow(
              title: 'Soundboard volume',
              value: prefs.volume,
              min: 0,
              max: 100,
              shown: '${prefs.volume} %',
              onChanged: (value) =>
                  notifier.update((prefs) => prefs.copyWith(volume: value)),
            ),
            SettingsSwitchRow(
              title: 'Mute all soundboards',
              value: prefs.muted,
              onChanged: (value) =>
                  notifier.update((prefs) => prefs.copyWith(muted: value)),
            ),
          ],
        ),
        SettingsSection(
          title: 'Voice sounds',
          footer: 'Short sounds in voice channels, heard only by you.',
          children: [
            for (final group in CueGroup.values)
              SettingsSwitchRow(
                title: group.label,
                value: !cuesOff.contains(group),
                onChanged: (on) =>
                    ref.read(cuePrefsProvider.notifier).set(group, on: on),
              ),
          ],
        ),
      ],
    );
  }
}
