import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/user_audio_prefs.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

/// The per-user voice controls of the member menu (desktop UI plan §17.3):
/// how loud someone is on this device, Mute and Mute soundboard. Only this
/// device hears the difference; the choice follows their identity to other
/// servers and is kept across restarts.
class UserAudioControls extends ConsumerWidget {
  const UserAudioControls({
    super.key,
    required this.serverKey,
    required this.userId,
  });

  final String serverKey;
  final int userId;

  /// The slider's steps.
  static const step = 5;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final who = (server: serverKey, user: userId);
    final key = ref.watch(userAudioKeyProvider(who));
    final audio = ref.watch(userAudioProvider(who));
    void change(UserAudio Function(UserAudio audio) edit) {
      if (key != null) {
        ref.read(userAudioPrefsProvider.notifier).update(key, edit);
      }
    }

    final shown = '${audio.volume}%';
    // The slider draws on a Material; the menu is a plain surface.
    return Material(
      type: MaterialType.transparency,
      child: SizedBox(
        width: 240,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 6, 10, 0),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      'User volume',
                      style: OcText.body.copyWith(color: colors.text),
                    ),
                  ),
                  Text(
                    shown,
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ],
              ),
            ),
            Slider(
              value: audio.volume.toDouble(),
              max: UserAudio.maxVolume.toDouble(),
              divisions: UserAudio.maxVolume ~/ step,
              label: shown,
              semanticFormatterCallback: (_) => shown,
              onChanged: key == null
                  ? null
                  : (raw) =>
                        change((audio) => audio.copyWith(volume: raw.round())),
            ),
            OcMenuToggle(
              label: 'Mute',
              checked: audio.muted,
              onChanged: key == null
                  ? null
                  : (muted) => change((audio) => audio.copyWith(muted: muted)),
            ),
            OcMenuToggle(
              label: 'Mute soundboard',
              checked: audio.soundboardMuted,
              onChanged: key == null
                  ? null
                  : (muted) => change(
                      (audio) => audio.copyWith(soundboardMuted: muted),
                    ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A watched screen share's sound on this device (Phase 2 plan §10.5):
/// its volume and mute, apart from the streamer's voice; kept with the
/// streamer's other choices.
class StreamAudioControls extends ConsumerWidget {
  const StreamAudioControls({
    super.key,
    required this.serverKey,
    required this.userId,
  });

  final String serverKey;
  final int userId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final who = (server: serverKey, user: userId);
    final key = ref.watch(userAudioKeyProvider(who));
    final audio = ref.watch(userAudioProvider(who));
    void change(UserAudio Function(UserAudio audio) edit) {
      if (key != null) {
        ref.read(userAudioPrefsProvider.notifier).update(key, edit);
      }
    }

    final shown = '${audio.streamVolume}%';
    return Material(
      type: MaterialType.transparency,
      child: SizedBox(
        width: 240,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 6, 10, 0),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      'Stream volume',
                      style: OcText.body.copyWith(color: colors.text),
                    ),
                  ),
                  Text(
                    shown,
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ],
              ),
            ),
            Slider(
              value: audio.streamVolume.toDouble(),
              max: UserAudio.maxVolume.toDouble(),
              divisions: UserAudio.maxVolume ~/ UserAudioControls.step,
              label: shown,
              semanticFormatterCallback: (_) => shown,
              onChanged: key == null
                  ? null
                  : (raw) => change(
                      (audio) => audio.copyWith(streamVolume: raw.round()),
                    ),
            ),
            OcMenuToggle(
              label: 'Mute stream',
              checked: audio.streamMuted,
              onChanged: key == null
                  ? null
                  : (muted) =>
                        change((audio) => audio.copyWith(streamMuted: muted)),
            ),
          ],
        ),
      ),
    );
  }
}
