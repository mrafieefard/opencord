import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/settings/user/settings_draft.dart';
import 'package:opencord/core/app_info.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/desktop/installation.dart';
import 'package:opencord/features/settings/user/keybind_recorder.dart';
import 'package:opencord/features/voice/camera_menu.dart';
import 'package:opencord/features/voice/input_level_meter.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Windows & behavior (§8.1, desktop only).
class BehaviorPage extends ConsumerWidget {
  const BehaviorPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(shownAppSettingsProvider);
    final update = ref.read(clientSettingsDraftProvider.notifier).updateApp;
    // Only Linux has a tray yet (D21); on a Mac, closing the window keeps
    // Opencord in the Dock anyway.
    final tray = Theme.of(context).platform == TargetPlatform.linux;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Window',
          children: [
            if (tray) ...[
              SettingsSwitchRow(
                title: 'Close button minimizes to the tray',
                subtitle: 'Messages keep arriving in the background',
                value: settings.closeToTray,
                onChanged: (value) =>
                    update((s) => s.copyWith(closeToTray: value)),
              ),
              SettingsSwitchRow(
                title: 'Start minimized',
                subtitle: 'When Opencord starts at login',
                value: settings.startMinimized,
                onChanged: (value) =>
                    update((s) => s.copyWith(startMinimized: value)),
              ),
            ],
            SettingsSwitchRow(
              title: 'Launch at login',
              value: settings.launchAtLogin,
              onChanged: (value) =>
                  update((s) => s.copyWith(launchAtLogin: value)),
            ),
            SettingsSwitchRow(
              title: 'Restore the last window position',
              value: settings.restoreWindowPosition,
              onChanged: (value) =>
                  update((s) => s.copyWith(restoreWindowPosition: value)),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        const _MediaCacheSection(),
      ],
    );
  }
}

class _MediaCacheSection extends ConsumerStatefulWidget {
  const _MediaCacheSection();

  @override
  ConsumerState<_MediaCacheSection> createState() => _MediaCacheSectionState();
}

class _MediaCacheSectionState extends ConsumerState<_MediaCacheSection> {
  int? _size;
  var _clearing = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  String? _error;

  Future<void> _load() async {
    try {
      final size = await ref.read(repositoryProvider).mediaCacheSize();
      if (mounted) setState(() => _size = size);
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
  }

  Future<void> _clear() async {
    setState(() => _clearing = true);
    final repository = ref.read(repositoryProvider);
    try {
      await repository.clearMediaCache();
      final size = await repository.mediaCacheSize();
      if (!mounted) return;
      setState(() => _size = size);
      showOcToast(context, 'Media cache cleared');
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    } finally {
      if (mounted) setState(() => _clearing = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final size = _size;
    return SettingsSection(
      title: 'Media cache',
      footer:
          'Thumbnails, pictures and voice messages you have seen, kept '
          'encrypted on this device, up to 1 GB. Saved files are not in it.',
      children: [
        SettingsRow(
          title: switch ((size, _error)) {
            (final size?, _) => formatFileSize(size),
            (null, final error?) => error,
            (null, null) => 'Calculating…',
          },
          trailing: OcButton(
            label: 'Clear',
            dense: true,
            busy: _clearing,
            onPressed: size != null && size > 0 ? _clear : null,
          ),
        ),
      ],
    );
  }
}

/// Voice & audio (§8.1, §17): devices, input mode, noise suppression and
/// volumes. The full page waits for the rest of §17.
class VoicePage extends ConsumerStatefulWidget {
  const VoicePage({super.key});

  @override
  ConsumerState<VoicePage> createState() => _VoicePageState();
}

class _VoicePageState extends ConsumerState<VoicePage> {
  @override
  void initState() {
    super.initState();
    // Devices plugged in since voice last looked.
    ref.read(audioDeviceListProvider.notifier).refresh();
  }

  @override
  Widget build(BuildContext context) {
    final audio = ref.watch(shownAudioProvider);
    final devices = ref.watch(audioDeviceListProvider);
    final update = ref.read(clientSettingsDraftProvider.notifier).updateAudio;
    Widget device(
      String? current,
      String? savedName,
      List<AudioDevice> choices,
      AudioSettings Function(AudioDevice? device) choose,
    ) => Builder(
      builder: (context) => OcButton(
        label: deviceLabel(current, savedName, choices),
        dense: true,
        icon: OcIcons.expandMore,
        onPressed: () {
          final rect = globalRectOf(context);
          showOcMenu(
            context: context,
            position: rect.bottomLeft.translate(0, 4),
            entries: [
              for (final (choice, label) in deviceChoices(choices))
                OcMenuItem(
                  label: label,
                  checked: choice?.id == current,
                  onSelected: () => update((_) => choose(choice)),
                ),
            ],
          );
        },
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Devices',
          children: [
            SettingsRow(
              title: 'Input device',
              icon: OcIcons.mic,
              trailing: device(
                audio.inputDevice,
                audio.inputDeviceName,
                devices.inputs,
                audio.withInput,
              ),
            ),
            SettingsRow(
              title: 'Output device',
              icon: OcIcons.headphones,
              trailing: device(
                audio.outputDevice,
                audio.outputDeviceName,
                devices.outputs,
                audio.withOutput,
              ),
            ),
            SettingsRow(
              title: 'Camera',
              icon: OcIcons.videocam,
              trailing: Builder(
                builder: (context) => OcButton(
                  label: ref.watch(chosenCameraProvider)?.$2 ?? 'Default',
                  dense: true,
                  icon: OcIcons.expandMore,
                  onPressed: () => showCameraMenu(
                    context,
                    ref,
                    globalRectOf(context).bottomLeft.translate(0, 4),
                  ),
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Input mode',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s12),
              child: SettingsChoiceCards<InputMode>(
                value: audio.inputMode,
                onChanged: (mode) => update((a) => a.copyWith(inputMode: mode)),
                options: const [
                  ChoiceCardOption(
                    value: InputMode.voiceActivity,
                    label: 'Voice activity',
                    description: 'Sends when you speak',
                  ),
                  ChoiceCardOption(
                    value: InputMode.pushToTalk,
                    label: 'Push to talk',
                    description: 'Sends while a key is held',
                  ),
                ],
              ),
            ),
            if (audio.inputMode == InputMode.pushToTalk) ...[
              const _PushToTalkKeys(),
              SettingsSliderRow(
                title: 'Release delay',
                value: audio.pushToTalkRelease.inMilliseconds,
                min: 0,
                max: AudioSettings.maxRelease.inMilliseconds,
                step: 20,
                shown: '${audio.pushToTalkRelease.inMilliseconds} ms',
                onChanged: (ms) => update(
                  (a) =>
                      a.copyWith(pushToTalkRelease: Duration(milliseconds: ms)),
                ),
              ),
            ],
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Noise suppression',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s12),
              child: SettingsChoiceCards<NoiseSuppression>(
                value: audio.noiseSuppression ?? NoiseSuppression.standard,
                onChanged: (mode) =>
                    update((a) => a.copyWith(noiseSuppression: mode)),
                options: const [
                  ChoiceCardOption(
                    value: NoiseSuppression.off,
                    label: 'Off',
                    description: 'Sends the microphone as it is',
                  ),
                  ChoiceCardOption(
                    value: NoiseSuppression.standard,
                    label: 'Standard',
                    description: 'Removes steady noise; very light',
                  ),
                  ChoiceCardOption(
                    value: NoiseSuppression.high,
                    label: 'High',
                    description: 'Also keyboards and dogs; heavier',
                  ),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Volume',
          children: [
            _VolumeRow(
              label: 'Input volume',
              value: audio.inputVolume,
              onChanged: (value) =>
                  update((a) => a.copyWith(inputVolume: value)),
              meter: true,
            ),
            _VolumeRow(
              label: 'Output volume',
              value: audio.outputVolume,
              onChanged: (value) =>
                  update((a) => a.copyWith(outputVolume: value)),
            ),
          ],
        ),
        // Lowering other apps while people talk (Phase 2 §12); macOS has
        // no way to.
        if (ref.read(repositoryProvider).capabilities.lowerOtherApps) ...[
          const SizedBox(height: OcSpace.s24),
          SettingsSection(
            title: 'Attenuation',
            footer:
                "Lowers other apps' sound while people talk in a call, and "
                'brings it back a second after.',
            children: [
              SettingsSliderRow(
                title: 'Lower other apps by',
                value: audio.attenuationPercent,
                min: 0,
                max: 100,
                shown: audio.attenuationPercent == 0
                    ? 'Off'
                    : '${audio.attenuationPercent} %',
                onChanged: (value) =>
                    update((a) => a.copyWith(attenuationPercent: value)),
              ),
              SettingsSwitchRow(
                title: 'When others speak',
                value: audio.attenuateWhenOthersSpeak,
                onChanged: (value) =>
                    update((a) => a.copyWith(attenuateWhenOthersSpeak: value)),
              ),
              SettingsSwitchRow(
                title: 'When I speak',
                value: audio.attenuateWhenISpeak,
                onChanged: (value) =>
                    update((a) => a.copyWith(attenuateWhenISpeak: value)),
              ),
            ],
          ),
        ],
        const SizedBox(height: OcSpace.s24),
        const _MusicListening(),
      ],
    );
  }
}

/// Music in voice channels (Phase 3 plan §19.8): one volume for every
/// server, kept on this device.
class _MusicListening extends ConsumerWidget {
  const _MusicListening();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final listening = ref.watch(shownMusicProvider);
    final draft = ref.read(clientSettingsDraftProvider.notifier);
    return SettingsSection(
      title: 'Music',
      footer:
          'For music in voice channels, on every server. Muted, the music '
          'is not sent to you at all.',
      children: [
        SettingsSliderRow(
          title: 'Music volume',
          value: listening.volume,
          min: 0,
          max: 200,
          step: 1,
          shown: '${listening.volume} %',
          onChanged: (value) =>
              draft.updateMusic((l) => l.copyWith(volume: value)),
        ),
        SettingsSwitchRow(
          title: 'Mute music',
          value: listening.muted,
          onChanged: (value) =>
              draft.updateMusic((l) => l.copyWith(muted: value)),
        ),
        SettingsSwitchRow(
          title: 'Lower music while people talk',
          subtitle: '10 dB quieter while someone speaks',
          value: listening.duckWhileTalking,
          onChanged: (value) =>
              draft.updateMusic((l) => l.copyWith(duckWhileTalking: value)),
        ),
      ],
    );
  }
}

/// The push-to-talk key, as Discord's voice settings show it: the first
/// push-to-talk keybind, recorded here; more in Keybinds.
class _PushToTalkKeys extends ConsumerWidget {
  const _PushToTalkKeys();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final keybinds = ref.watch(
      shownKeybindsProvider.select((k) => k.of(HotkeyAction.pushToTalk)),
    );
    final first = keybinds.firstOrNull;
    final draft = ref.read(clientSettingsDraftProvider.notifier);
    final more = keybinds.length - 1;
    return SettingsRow(
      title: 'Shortcut',
      subtitle: more > 0
          ? 'And $more more in Keybinds'
          : 'Add more keys in Keybinds',
      trailing: SizedBox(
        width: 220,
        child: KeybindRecorder(
          keys: first?.keys ?? '',
          semanticLabel: 'Push-to-talk keys',
          onRecorded: (keys) => first == null
              ? draft.keybindAdd(HotkeyAction.pushToTalk, keys: keys)
              : draft.keybindChange(
                  first.id,
                  (row) => row.copyWith(keys: keys),
                ),
        ),
      ),
    );
  }
}

class _VolumeRow extends StatelessWidget {
  const _VolumeRow({
    required this.label,
    required this.value,
    required this.onChanged,
    this.meter = false,
  });

  final String label;
  final int value;
  final ValueChanged<int> onChanged;

  /// The microphone's live level under the slider (§17.1).
  final bool meter;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        OcSpace.s16,
        OcSpace.s10,
        OcSpace.s16,
        OcSpace.s4,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  label,
                  style: OcText.body.copyWith(color: colors.text),
                ),
              ),
              Text(
                '$value%',
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ],
          ),
          Slider(
            value: value.toDouble(),
            max: AudioSettings.maxVolume.toDouble(),
            divisions: AudioSettings.maxVolume,
            semanticFormatterCallback: (value) => '${value.round()}%',
            onChanged: (value) => onChanged(value.round()),
          ),
          if (meter) ...[
            const InputLevelMeter(),
            const SizedBox(height: OcSpace.s8),
          ],
        ],
      ),
    );
  }
}

/// Notifications (§8.1).
class NotificationsPage extends ConsumerWidget {
  const NotificationsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(shownAppSettingsProvider);
    final update = ref.read(clientSettingsDraftProvider.notifier).updateApp;
    return SettingsSection(
      title: 'Notifications',
      footer:
          'Muted servers and channels never notify. Mute them from their '
          'menus in the sidebar.',
      children: [
        SettingsSwitchRow(
          title: 'Desktop notifications',
          value: settings.desktopNotifications,
          onChanged: (value) =>
              update((s) => s.copyWith(desktopNotifications: value)),
        ),
        SettingsSwitchRow(
          title: 'Only for mentions',
          subtitle: 'Notify when someone writes @you, not for every message',
          value: settings.mentionsOnly,
          onChanged: settings.desktopNotifications
              ? (value) => update((s) => s.copyWith(mentionsOnly: value))
              : null,
        ),
        SettingsSwitchRow(
          title: 'Message sound',
          value: settings.messageSound,
          onChanged: (value) => update((s) => s.copyWith(messageSound: value)),
        ),
        SettingsSwitchRow(
          title: 'Flash the taskbar',
          subtitle: 'When a message arrives while Opencord is in the back',
          value: settings.flashTaskbar,
          onChanged: (value) => update((s) => s.copyWith(flashTaskbar: value)),
        ),
      ],
    );
  }
}

/// Trusted servers (§8.1): every server with its pinned fingerprint.
class TrustedServersPage extends ConsumerWidget {
  const TrustedServersPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final servers = ref.watch(serverListProvider);
    if (servers.isEmpty) {
      return Text(
        'No servers yet.',
        style: OcText.body.copyWith(color: colors.textMuted),
      );
    }
    return SettingsSection(
      title: 'Servers',
      footer:
          'Opencord connects only when a server shows the fingerprint saved '
          'here. Forgetting a server removes it and its fingerprint; adding '
          'it again asks you to verify it.',
      children: [
        for (final server in servers)
          SettingsRow(
            title: '${server.name} · ${server.key}',
            subtitle: server.fingerprint == null
                ? 'Trusted through a public certificate'
                : groupedFingerprint(server.fingerprint!),
            mono: server.fingerprint != null,
            trailing: OcButton.ghost(
              label: 'Forget',
              dense: true,
              onPressed: () async {
                final forget = await confirmAction(
                  context,
                  title: 'Forget ${server.name}?',
                  message:
                      '${server.name} and its fingerprint are removed from '
                      'this device. You stay a member.',
                  action: 'Forget server',
                );
                if (!forget) return;
                try {
                  await ref.read(repositoryProvider).removeServer(server.key);
                } on RepoException catch (error) {
                  if (context.mounted) showOcToast(context, error.message);
                }
              },
            ),
          ),
      ],
    );
  }
}

/// About (§8.1): versions, licenses and where the code lives.
class AboutPage extends ConsumerWidget {
  const AboutPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final installation = ref.watch(installationProvider);
    final version = installation?.version ?? appVersion;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text('Opencord', style: OcText.title.copyWith(color: colors.text)),
        const SizedBox(height: OcSpace.s4),
        Text(
          'Self-hosted chat and voice.',
          style: OcText.body.copyWith(color: colors.textSecondary),
        ),
        const SizedBox(height: OcSpace.s20),
        if (_desktop(Theme.of(context).platform)) ...[
          const _UpdatesSection(),
          const SizedBox(height: OcSpace.s24),
        ],
        SettingsSection(
          children: [
            SettingsRow(title: 'App version', subtitle: version, mono: true),
            SettingsRow(
              title: 'Core version',
              subtitle: ref.watch(coreVersionProvider),
              mono: true,
            ),
            const SettingsRow(
              title: 'License',
              subtitle: 'App: MPL-2.0 · Server: AGPL-3.0',
            ),
            SettingsRow(
              title: 'Open-source licenses',
              subtitle: 'Fonts, icons, emoji data and libraries',
              onTap: () => showLicensePage(
                context: context,
                applicationName: 'Opencord',
                applicationVersion: version,
              ),
            ),
          ],
        ),
      ],
    );
  }
}

bool _desktop(TargetPlatform platform) =>
    platform == TargetPlatform.linux ||
    platform == TargetPlatform.windows ||
    platform == TargetPlatform.macOS;

/// The update track (docs/updater.md): the launcher checks GitHub for the
/// newest build of it each time Opencord starts.
class _UpdatesSection extends ConsumerWidget {
  const _UpdatesSection();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final installation = ref.watch(installationProvider);
    if (installation == null) {
      return const SettingsSection(
        title: 'Updates',
        children: [
          SettingsRow(
            title: "This copy doesn't update itself",
            subtitle:
                'It was not installed with the Opencord installer. Install '
                'Opencord from its releases page to get updates.',
          ),
        ],
      );
    }
    final track = ref.watch(releaseTrackProvider);
    return SettingsSection(
      title: 'Updates',
      footer:
          'Opencord checks for updates each time it starts. Switching tracks '
          "restarts it on that track's newest build.",
      children: [
        Padding(
          padding: const EdgeInsets.all(OcSpace.s12),
          child: SettingsChoiceCards<ReleaseTrack>(
            value: track ?? ReleaseTrack.stable,
            onChanged: track == null
                ? null
                : (chosen) => _switchTrack(context, ref, installation, chosen),
            options: [
              for (final option in ReleaseTrack.values)
                ChoiceCardOption(
                  value: option,
                  label: option.label,
                  description: option.description,
                ),
            ],
          ),
        ),
        if (installation.launcher != null)
          SettingsRow(
            title: 'Check for updates now',
            subtitle: 'Restarts Opencord',
            trailing: OcButton(
              label: 'Restart',
              dense: true,
              onPressed: installation.restart,
            ),
          ),
      ],
    );
  }
}

/// Asks before switching to [chosen], then restarts on it: the launcher
/// installs that track's newest build as the app starts again.
Future<void> _switchTrack(
  BuildContext context,
  WidgetRef ref,
  Installation installation,
  ReleaseTrack chosen,
) async {
  if (chosen == ref.read(releaseTrackProvider)) return;
  final restarts = installation.launcher != null;
  final confirmed = await confirmAction(
    context,
    title: 'Switch to ${chosen.label}?',
    message: restarts
        ? 'Opencord restarts and opens on the newest ${chosen.label} build. '
              'It may be older than the one you have now.'
        : 'Opencord opens on the newest ${chosen.label} build the next time '
              'it starts. It may be older than the one you have now.',
    action: restarts ? 'Restart' : 'Switch',
  );
  if (!confirmed) return;
  try {
    await ref.read(releaseTrackProvider.notifier).set(chosen);
  } on FileSystemException catch (error) {
    if (context.mounted) {
      showOcToast(context, "Couldn't switch tracks: ${error.message}");
    }
    return;
  }
  if (restarts) await installation.restart();
}
