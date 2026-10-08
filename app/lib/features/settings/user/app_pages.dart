import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/app_info.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/voice/input_level_meter.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
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
    final settings = ref.watch(appSettingsProvider);
    final update = ref.read(appSettingsProvider.notifier).update;
    // Only Linux has a tray yet (D21); on a Mac, closing the window keeps
    // Opencord in the Dock anyway.
    final tray = Theme.of(context).platform == TargetPlatform.linux;
    return SettingsSection(
      title: 'Window',
      children: [
        if (tray) ...[
          SettingsSwitchRow(
            title: 'Close button minimizes to the tray',
            subtitle: 'Messages keep arriving in the background',
            value: settings.closeToTray,
            onChanged: (value) => update((s) => s.copyWith(closeToTray: value)),
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
          onChanged: (value) => update((s) => s.copyWith(launchAtLogin: value)),
        ),
        SettingsSwitchRow(
          title: 'Restore the last window position',
          value: settings.restoreWindowPosition,
          onChanged: (value) =>
              update((s) => s.copyWith(restoreWindowPosition: value)),
        ),
      ],
    );
  }
}

/// Voice & audio (§8.1, §17): devices, input mode and volumes. The full
/// page waits for the rest of §17.
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
    final audio = ref.watch(audioSettingsProvider);
    final devices = ref.watch(audioDeviceListProvider);
    final update = ref.read(audioSettingsProvider.notifier).update;
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
      ],
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
            divisions: AudioSettings.maxVolume ~/ 5,
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
    final settings = ref.watch(appSettingsProvider);
    final update = ref.read(appSettingsProvider.notifier).update;
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

/// Keybinds (§7, §8.1): what each shortcut does on this platform.
class KeybindsPage extends StatelessWidget {
  const KeybindsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final platform = Theme.of(context).platform;
    final mac = platform == TargetPlatform.macOS;
    SingleActivator primary(
      LogicalKeyboardKey key, {
      bool shift = false,
      bool alt = false,
    }) => appShortcut(key, platform, shift: shift, alt: alt);
    const up = LogicalKeyboardKey.arrowUp;
    const down = LogicalKeyboardKey.arrowDown;
    final rows = <(String, List<SingleActivator>)>[
      ('Quick switcher', [primary(LogicalKeyboardKey.keyK)]),
      ('User settings', [primary(LogicalKeyboardKey.comma)]),
      (
        'Previous / next channel',
        [
          const SingleActivator(up, alt: true),
          const SingleActivator(down, alt: true),
        ],
      ),
      (
        'Previous / next unread channel',
        [
          const SingleActivator(up, alt: true, shift: true),
          const SingleActivator(down, alt: true, shift: true),
        ],
      ),
      (
        'Previous / next server',
        [primary(up, alt: true), primary(down, alt: true)],
      ),
      ('Toggle mute', [primary(LogicalKeyboardKey.keyM, shift: true)]),
      ('Toggle deafen', [primary(LogicalKeyboardKey.keyD, shift: true)]),
      ('Toggle member list', [primary(LogicalKeyboardKey.keyU, shift: true)]),
      (
        'Send / new line',
        [
          const SingleActivator(LogicalKeyboardKey.enter),
          const SingleActivator(LogicalKeyboardKey.enter, shift: true),
        ],
      ),
      ('Edit your last message (empty composer)', [const SingleActivator(up)]),
      (
        'Cancel a reply or edit, close, mark read',
        [const SingleActivator(LogicalKeyboardKey.escape)],
      ),
      (
        'Scroll messages',
        [
          const SingleActivator(LogicalKeyboardKey.pageUp),
          const SingleActivator(LogicalKeyboardKey.pageDown),
        ],
      ),
      (
        'Fullscreen',
        [
          if (mac)
            const SingleActivator(
              LogicalKeyboardKey.keyF,
              control: true,
              meta: true,
            )
          else
            const SingleActivator(LogicalKeyboardKey.f11),
        ],
      ),
    ];
    final colors = context.oc;
    return SettingsSection(
      title: 'Shortcuts',
      footer:
          'Double-click a message to reply. Changing shortcuts comes in a later version.',
      children: [
        for (final (action, keys) in rows)
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s16,
              vertical: OcSpace.s12,
            ),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    action,
                    style: OcText.body.copyWith(color: colors.text),
                  ),
                ),
                Wrap(
                  spacing: OcSpace.s4,
                  children: [for (final key in keys) KeyHint(key)],
                ),
              ],
            ),
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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text('Opencord', style: OcText.title.copyWith(color: colors.text)),
        const SizedBox(height: OcSpace.s4),
        Text(
          'Self-hosted chat and voice, with Telegram’s calm and '
          'Discord’s servers.',
          style: OcText.body.copyWith(color: colors.textSecondary),
        ),
        const SizedBox(height: OcSpace.s20),
        SettingsSection(
          children: [
            SettingsRow(title: 'App version', subtitle: appVersion, mono: true),
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
                applicationVersion: appVersion,
              ),
            ),
          ],
        ),
      ],
    );
  }
}
