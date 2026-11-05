import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/dialogs/create_channel_dialog.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/servers/server_rail.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_float.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The server header float (§5.3), 52 px tall.
class ServerHead extends ConsumerStatefulWidget {
  const ServerHead({super.key});

  @override
  ConsumerState<ServerHead> createState() => _ServerHeadState();
}

class _ServerHeadState extends ConsumerState<ServerHead> {
  bool _menuOpen = false;

  Future<void> _showMenu(
    BuildContext context,
    WidgetRef ref,
    String name,
    String serverKey,
  ) async {
    final muted = ref.read(notificationPrefsProvider).serverMuted(serverKey);
    final data = ref.read(serverProvider(serverKey)).data;
    final invite = data?.can(Permissions.createInvite) ?? false;
    final manage = data?.can(Permissions.manageChannels) ?? false;
    final rect = globalRectOf(context);
    setState(() => _menuOpen = true);
    try {
      await showOcMenu(
        context: context,
        key: ShellKeys.srvMenu,
        position: Offset(rect.left, rect.bottom + 6),
        width: math.min(rect.width, 288.0),
        entries: [
          if (invite)
            OcMenuItem(
              label: 'Invite people',
              icon: OcIcons.personAdd,
              onSelected: () => showInvitePeople(context, serverKey: serverKey),
            ),
          if (manage) ...[
            OcMenuItem(
              label: 'Create channel',
              icon: OcIcons.add,
              onSelected: () =>
                  showCreateChannel(context, serverKey: serverKey),
            ),
            OcMenuItem(
              label: 'Create category',
              icon: OcIcons.createNewFolder,
              onSelected: () =>
                  showCreateCategory(context, serverKey: serverKey),
            ),
          ],
          if (canOpenServerSettings(data))
            OcMenuItem(
              label: 'Server settings',
              icon: OcIcons.settings,
              onSelected: () =>
                  showServerSettings(context, ref, serverKey: serverKey),
            ),
          if (invite || manage || canOpenServerSettings(data))
            const OcMenuDivider(),
          OcMenuItem(
            label: muted ? 'Unmute notifications' : 'Mute notifications',
            icon: muted ? OcIcons.notifications : OcIcons.notificationsOff,
            onSelected: () => ref
                .read(notificationPrefsProvider.notifier)
                .toggleServer(serverKey),
          ),
          OcMenuItem(
            label: 'Copy server address',
            icon: OcIcons.link,
            onSelected: () {
              Clipboard.setData(ClipboardData(text: serverKey));
              showOcToast(context, 'Address copied');
            },
          ),
          const OcMenuDivider(),
          OcMenuItem(
            label: 'Leave server',
            icon: OcIcons.logout,
            onSelected: () => confirmLeaveServer(context, ref, serverKey, name),
          ),
        ],
      );
    } finally {
      if (mounted) setState(() => _menuOpen = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final homeShown = ref.watch(homeShownProvider);
    final serverKey = homeShown ? null : ref.watch(currentServerProvider);

    final String name;
    final String status;
    final bool isConnected;
    final bool isButton = serverKey != null;

    if (serverKey == null) {
      name = 'Private messages';
      final contacts = ref.watch(privateContactsProvider);
      final online = contacts.friends.where((c) {
        final presence = ref.watch(contactPresenceProvider(c.peer));
        return presence != Presence.offline;
      }).length;
      status = '${countLabel(online)} online';
      isConnected = true;
    } else {
      final state = ref.watch(serverProvider(serverKey));
      final online = ref.watch(
        presenceProvider(serverKey).select(
          (p) => p.presences.values
              .where((presence) => presence != Presence.offline)
              .length,
        ),
      );
      name =
          state.data?.info.name ??
          ref
              .watch(serverListProvider)
              .where((s) => s.key == serverKey)
              .firstOrNull
              ?.name ??
          serverKey;
      final members = state.data?.members.length ?? 0;
      status = switch (state.connection.phase) {
        ConnectionPhase.connected =>
          '${countLabel(online)} online · ${countLabel(members)} members',
        ConnectionPhase.connecting => 'Connecting…',
        ConnectionPhase.reconnecting => 'Reconnecting…',
        ConnectionPhase.failed => 'Connection failed',
      };
      isConnected = state.connection.isConnected;
    }

    return SizedBox(
      height: OcFloats.header,
      child: Hoverable(
        onTap: isButton ? () => _showMenu(context, ref, name, serverKey) : null,
        semanticLabel: isButton
            ? '$name, $status. Server menu'
            : '$name, $status',
        cursor: isButton ? SystemMouseCursors.click : SystemMouseCursors.basic,
        builder: (context, hover) {
          final isHovered = isButton && hover.active;
          return OcFloat(
            key: ShellKeys.head,
            color: isHovered ? colors.hover : colors.sidebar,
            duration: OcMotion.of(context).hover,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(14, 0, 10, 0),
              child: Row(
                children: [
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: OcText.header.copyWith(
                            fontSize: 15,
                            fontWeight: FontWeight.w600,
                            height: 1.25,
                            color: colors.text,
                          ),
                        ),
                        Row(
                          children: [
                            ConnectionDot(connected: isConnected, size: 6),
                            const SizedBox(width: 6),
                            Expanded(
                              child: Text(
                                status,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: OcText.meta.copyWith(
                                  fontSize: 12,
                                  height: 1.25,
                                  color: colors.textMuted,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                  if (isButton) ...[
                    const SizedBox(width: 8),
                    AnimatedRotation(
                      turns: _menuOpen ? 0.5 : 0,
                      duration: OcMotion.of(context).turn,
                      curve: Curves.easeOut,
                      child: Icon(
                        OcIcons.expandMore,
                        size: 18,
                        color: colors.textMuted,
                      ),
                    ),
                  ],
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}
