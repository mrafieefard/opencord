import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/settings/user/settings_draft.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/chat/links.dart';
import 'package:opencord/features/settings/user/keybind_recorder.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popup_route.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// User settings → Keybinds (§8.1), like Discord's: keybinds anyone can
/// add, each an action and the keys that do it — several per action if
/// wanted, push-to-talk included — then the app's own shortcuts.
class KeybindsPage extends ConsumerStatefulWidget {
  const KeybindsPage({super.key});

  @override
  ConsumerState<KeybindsPage> createState() => _KeybindsPageState();
}

class _KeybindsPageState extends ConsumerState<KeybindsPage> {
  /// The row just added, which starts recording.
  int? _added;

  @override
  Widget build(BuildContext context) {
    final keybinds = ref.watch(shownKeybindsProvider);
    final support = ref.watch(hotkeySupportProvider);
    final draft = ref.read(clientSettingsDraftProvider.notifier);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Keybinds',
          footer: switch (support) {
            HotkeysGlobal(:final method) =>
              'Keybinds work in other apps too, through $method.',
            HotkeysFocusedOnly(:final reason) =>
              'Keybinds work only while Opencord is focused: $reason.',
            null => null,
          },
          children: [
            if (support case HotkeysFocusedOnly(:final settingsUrl?))
              SettingsRow(
                title: 'Allow keybinds in other apps',
                subtitle:
                    'Opencord tries again when you come back to this window.',
                trailing: OcButton(
                  label: 'Open settings',
                  dense: true,
                  onPressed: () => _openSettings(ref, settingsUrl),
                ),
              ),
            if (keybinds.list.isEmpty)
              const SettingsRow(
                title: 'No keybinds yet',
                subtitle:
                    'Add one to push to talk, mute or deafen with a key, '
                    'even from another app.',
              ),
            for (final keybind in keybinds.list)
              _KeybindRow(
                key: ValueKey(keybind.id),
                keybind: keybind,
                sharing: keybinds.sharingKeys(keybind),
                record: keybind.id == _added,
              ),
            Padding(
              padding: const EdgeInsets.all(OcSpace.s12),
              child: Align(
                alignment: Alignment.centerLeft,
                child: OcButton(
                  label: 'Add a keybind',
                  icon: OcIcons.add,
                  dense: true,
                  onPressed: () => setState(
                    () => _added = draft.keybindAdd(HotkeyAction.pushToTalk),
                  ),
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        const _AppShortcuts(),
      ],
    );
  }
}

class _KeybindRow extends ConsumerWidget {
  const _KeybindRow({
    super.key,
    required this.keybind,
    required this.sharing,
    required this.record,
  });

  final Keybind keybind;

  /// Other keybinds on the same keys.
  final List<Keybind> sharing;

  /// Starts recording when it appears.
  final bool record;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final draft = ref.read(clientSettingsDraftProvider.notifier);
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        OcSpace.s12,
        OcSpace.s8,
        OcSpace.s8,
        OcSpace.s8,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              SizedBox(
                width: 200,
                child: Builder(
                  builder: (context) => Hoverable(
                    semanticLabel: 'Action: ${keybind.action.label}',
                    focusRadius: BorderRadius.circular(8),
                    onTap: () => showOcMenu(
                      context: context,
                      position: globalRectOf(
                        context,
                      ).bottomLeft.translate(0, 4),
                      entries: [
                        for (final action in HotkeyAction.values)
                          OcMenuItem(
                            label: action.label,
                            checked: action == keybind.action,
                            onSelected: () => draft.keybindChange(
                              keybind.id,
                              (row) => row.copyWith(action: action),
                            ),
                          ),
                      ],
                    ),
                    builder: (context, state) => Container(
                      height: 36,
                      padding: const EdgeInsets.only(left: 12, right: 8),
                      decoration: BoxDecoration(
                        color: state.active ? colors.hover : colors.surface,
                        borderRadius: BorderRadius.circular(8),
                        border: Border.all(color: colors.border),
                      ),
                      child: Row(
                        children: [
                          Expanded(
                            child: Text(
                              keybind.action.label,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.body.copyWith(color: colors.text),
                            ),
                          ),
                          Icon(
                            OcIcons.expandMore,
                            size: 18,
                            color: colors.textSecondary,
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
              const SizedBox(width: OcSpace.s8),
              Expanded(
                child: KeybindRecorder(
                  keys: keybind.keys,
                  autofocus: record,
                  semanticLabel: 'Keys for ${keybind.action.label}',
                  onRecorded: (keys) => draft.keybindChange(
                    keybind.id,
                    (row) => row.copyWith(keys: keys),
                  ),
                ),
              ),
              const SizedBox(width: OcSpace.s4),
              OcIconButton(
                icon: OcIcons.close,
                tooltip: 'Remove keybind',
                onPressed: () => draft.keybindRemove(keybind.id),
              ),
            ],
          ),
          if (sharing.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: OcSpace.s4, left: 208),
              child: Text(
                'Also ${{for (final other in sharing) other.action.label}.join(', ')}',
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            ),
        ],
      ),
    );
  }
}

/// The app's own shortcuts (§7), which work while Opencord is focused.
class _AppShortcuts extends StatelessWidget {
  const _AppShortcuts();

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
      ('Open the soundboard', [primary(LogicalKeyboardKey.keyB, shift: true)]),
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
          'These work while Opencord is focused. Double-click a message to reply.',
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

/// Opens the system settings where global hotkeys are allowed.
void _openSettings(WidgetRef ref, String url) {
  final uri = Uri.tryParse(url);
  if (uri != null) unawaited(ref.read(linkLauncherProvider)(uri));
}
