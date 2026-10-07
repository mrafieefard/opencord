import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/dialogs/add_server_dialog.dart';
import 'package:opencord/features/dialogs/create_channel_dialog.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/features/settings/user_settings.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shortcuts.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/widgets/key_hint.dart';

/// The macOS menu bar (§15): Opencord, Edit, View, Server, Window and
/// Help, with the shortcuts of §7. Items run the shell's own actions, so a
/// menu item and its shortcut do the same thing; elsewhere this is just
/// [child]. Put it under the shell's [Actions].
class MacMenuBar extends ConsumerWidget {
  const MacMenuBar({super.key, required this.child});

  final Widget child;

  static const _mac = TargetPlatform.macOS;

  static SingleActivator _command(
    LogicalKeyboardKey key, {
    bool shift = false,
    bool alt = false,
  }) => appShortcut(key, _mac, shift: shift, alt: alt);

  /// Text editing goes to whatever has focus.
  static void _edit(Intent intent) {
    final focused = primaryFocus?.context;
    if (focused != null) Actions.maybeInvoke(focused, intent);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (Theme.of(context).platform != _mac) return child;
    final server = ref.watch(currentServerProvider);
    final data = server == null
        ? null
        : ref.watch(serverProvider(server).select((state) => state.data));
    final voice = ref.watch(voiceSessionProvider);
    final members = ref.watch(memberPanelProvider);
    final invite = data?.can(Permissions.createInvite) ?? false;
    final manage = data?.can(Permissions.manageChannels) ?? false;

    VoidCallback run(Intent intent) =>
        () => Actions.maybeInvoke(context, intent);
    VoidCallback? onServer(bool allowed, void Function(String server) open) =>
        allowed && server != null ? () => open(server) : null;

    PlatformMenuItem item(
      String label,
      VoidCallback? onSelected, [
      SingleActivator? shortcut,
    ]) => PlatformMenuItem(
      label: label,
      shortcut: shortcut,
      onSelected: onSelected,
    );

    const selection = SelectionChangedCause.keyboard;
    return PlatformMenuBar(
      menus: [
        PlatformMenu(
          label: 'Opencord',
          menus: [
            PlatformMenuItemGroup(
              members: [
                item(
                  'About Opencord',
                  () => showUserSettings(context, page: 'about'),
                ),
              ],
            ),
            PlatformMenuItemGroup(
              members: [
                item(
                  'Settings…',
                  run(const UserSettingsIntent()),
                  _command(LogicalKeyboardKey.comma),
                ),
              ],
            ),
            if (PlatformProvidedMenuItem.hasMenu(
              PlatformProvidedMenuItemType.servicesSubmenu,
            ))
              const PlatformMenuItemGroup(
                members: [
                  PlatformProvidedMenuItem(
                    type: PlatformProvidedMenuItemType.servicesSubmenu,
                  ),
                ],
              ),
            const PlatformMenuItemGroup(
              members: [
                PlatformProvidedMenuItem(
                  type: PlatformProvidedMenuItemType.hide,
                ),
                PlatformProvidedMenuItem(
                  type: PlatformProvidedMenuItemType.hideOtherApplications,
                ),
                PlatformProvidedMenuItem(
                  type: PlatformProvidedMenuItemType.showAllApplications,
                ),
              ],
            ),
            const PlatformMenuItemGroup(
              members: [
                PlatformProvidedMenuItem(
                  type: PlatformProvidedMenuItemType.quit,
                ),
              ],
            ),
          ],
        ),
        PlatformMenu(
          label: 'Edit',
          menus: [
            PlatformMenuItemGroup(
              members: [
                item(
                  'Undo',
                  () => _edit(const UndoTextIntent(selection)),
                  _command(LogicalKeyboardKey.keyZ),
                ),
                item(
                  'Redo',
                  () => _edit(const RedoTextIntent(selection)),
                  _command(LogicalKeyboardKey.keyZ, shift: true),
                ),
              ],
            ),
            PlatformMenuItemGroup(
              members: [
                item(
                  'Cut',
                  () => _edit(const CopySelectionTextIntent.cut(selection)),
                  _command(LogicalKeyboardKey.keyX),
                ),
                item(
                  'Copy',
                  () => _edit(CopySelectionTextIntent.copy),
                  _command(LogicalKeyboardKey.keyC),
                ),
                item(
                  'Paste',
                  () => _edit(const PasteTextIntent(selection)),
                  _command(LogicalKeyboardKey.keyV),
                ),
                item(
                  'Select All',
                  () => _edit(const SelectAllTextIntent(selection)),
                  _command(LogicalKeyboardKey.keyA),
                ),
              ],
            ),
          ],
        ),
        PlatformMenu(
          label: 'View',
          menus: [
            PlatformMenuItemGroup(
              members: [
                item(
                  'Quick Switcher',
                  run(const QuickSwitcherIntent()),
                  _command(LogicalKeyboardKey.keyK),
                ),
                item(
                  members ? 'Hide Member List' : 'Show Member List',
                  run(const ToggleMembersIntent()),
                  _command(LogicalKeyboardKey.keyU, shift: true),
                ),
              ],
            ),
            const PlatformMenuItemGroup(
              members: [
                PlatformProvidedMenuItem(
                  type: PlatformProvidedMenuItemType.toggleFullScreen,
                ),
              ],
            ),
          ],
        ),
        PlatformMenu(
          label: 'Server',
          menus: [
            PlatformMenuItemGroup(
              members: [
                item(
                  'Invite People…',
                  onServer(
                    invite,
                    (key) => showInvitePeople(context, serverKey: key),
                  ),
                ),
                item(
                  'Create Channel…',
                  onServer(
                    manage,
                    (key) => showCreateChannel(context, serverKey: key),
                  ),
                ),
                item(
                  'Create Category…',
                  onServer(
                    manage,
                    (key) => showCreateCategory(context, serverKey: key),
                  ),
                ),
                item(
                  'Server Settings…',
                  onServer(
                    canOpenServerSettings(data),
                    (key) => showServerSettings(context, ref, serverKey: key),
                  ),
                ),
              ],
            ),
            PlatformMenuItemGroup(
              members: [
                item(
                  'Previous Channel',
                  run(const ChannelStepIntent(-1)),
                  const SingleActivator(LogicalKeyboardKey.arrowUp, alt: true),
                ),
                item(
                  'Next Channel',
                  run(const ChannelStepIntent(1)),
                  const SingleActivator(
                    LogicalKeyboardKey.arrowDown,
                    alt: true,
                  ),
                ),
                item(
                  'Previous Unread Channel',
                  run(const UnreadStepIntent(-1)),
                  const SingleActivator(
                    LogicalKeyboardKey.arrowUp,
                    alt: true,
                    shift: true,
                  ),
                ),
                item(
                  'Next Unread Channel',
                  run(const UnreadStepIntent(1)),
                  const SingleActivator(
                    LogicalKeyboardKey.arrowDown,
                    alt: true,
                    shift: true,
                  ),
                ),
                item(
                  'Previous Server',
                  run(const ServerStepIntent(-1)),
                  _command(LogicalKeyboardKey.arrowUp, alt: true),
                ),
                item(
                  'Next Server',
                  run(const ServerStepIntent(1)),
                  _command(LogicalKeyboardKey.arrowDown, alt: true),
                ),
              ],
            ),
            PlatformMenuItemGroup(
              members: [
                item(
                  voice.muted ? 'Unmute' : 'Mute',
                  run(const ToggleMuteIntent()),
                  _command(LogicalKeyboardKey.keyM, shift: true),
                ),
                item(
                  voice.deafened ? 'Undeafen' : 'Deafen',
                  run(const ToggleDeafenIntent()),
                  _command(LogicalKeyboardKey.keyD, shift: true),
                ),
              ],
            ),
            PlatformMenuItemGroup(
              members: [item('Add Server…', () => showAddServer(context))],
            ),
          ],
        ),
        PlatformMenu(
          label: 'Window',
          menus: [
            PlatformMenuItemGroup(
              members: [
                item(
                  'Close Window',
                  () => ref.read(nativeWindowProvider).close(),
                  _command(LogicalKeyboardKey.keyW),
                ),
                const PlatformProvidedMenuItem(
                  type: PlatformProvidedMenuItemType.minimizeWindow,
                ),
                const PlatformProvidedMenuItem(
                  type: PlatformProvidedMenuItemType.zoomWindow,
                ),
              ],
            ),
            const PlatformMenuItemGroup(
              members: [
                PlatformProvidedMenuItem(
                  type: PlatformProvidedMenuItemType.arrangeWindowsInFront,
                ),
              ],
            ),
          ],
        ),
        PlatformMenu(
          label: 'Help',
          menus: [
            item(
              'Keyboard Shortcuts',
              () => showUserSettings(context, page: 'keybinds'),
            ),
          ],
        ),
      ],
      child: child,
    );
  }
}
