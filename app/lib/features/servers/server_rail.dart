import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/dialogs/add_server_dialog.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/settings/user_settings.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/drag_area.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The server rail (§4.1): one icon per server with its name underneath,
/// Telegram-folder style, reordered by dragging.
class ServerRail extends ConsumerWidget {
  const ServerRail({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final servers = ref.watch(serverListProvider);
    // macOS keeps its traffic lights above the rail (§3.1).
    final trafficLights =
        Theme.of(context).platform == TargetPlatform.macOS &&
        ref.watch(windowChromeProvider) == WindowChrome.custom &&
        !ref.watch(windowStatusProvider.select((status) => status.fullscreen));
    return ColoredBox(
      color: colors.rail,
      child: Column(
        children: [
          if (trafficLights)
            const SizedBox(height: 52, child: WindowDragArea()),
          SizedBox(
            height: OcSize.header,
            child: Stack(
              alignment: Alignment.center,
              children: [
                const Positioned.fill(child: WindowDragArea()),
                OcIconButton(
                  icon: OcIcons.menu,
                  tooltip: 'Settings',
                  onPressed: () => showUserSettings(context),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.only(bottom: OcSpace.s8),
            child: SizedBox(
              width: 32,
              child: Divider(height: 1, color: colors.border),
            ),
          ),
          Expanded(
            child: ReorderableListView.builder(
              padding: EdgeInsets.zero,
              buildDefaultDragHandles: false,
              itemCount: servers.length,
              proxyDecorator: (child, index, animation) =>
                  Opacity(opacity: 0.9, child: child),
              onReorderItem: (from, to) => ref
                  .read(serverListProvider.notifier)
                  .move(servers[from].key, to),
              itemBuilder: (context, index) => ReorderableDragStartListener(
                key: ValueKey(servers[index].key),
                index: index,
                child: _RailEntry(server: servers[index]),
              ),
            ),
          ),
          const SizedBox(height: OcSpace.s4),
          OcIconButton(
            icon: OcIcons.add,
            tooltip: 'Add server',
            onPressed: () => showAddServer(context),
          ),
          const SizedBox(height: OcSpace.s12),
        ],
      ),
    );
  }
}

/// Where a server's connection stands, as the rail shows it (§5.3).
enum RailConnection { connected, connecting, failed }

String _stateLabel(ConnectionStatus status) => switch (status.phase) {
  ConnectionPhase.connected => 'Connected',
  ConnectionPhase.connecting => 'Connecting…',
  ConnectionPhase.reconnecting => 'Reconnecting…',
  ConnectionPhase.failed => 'Connection failed',
};

class _RailEntry extends ConsumerWidget {
  const _RailEntry({required this.server});

  final ServerSummary server;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final key = server.key;
    final selected = ref.watch(currentServerProvider) == key;
    final status = ref.watch(serverProvider(key).select((s) => s.connection));
    final name =
        ref.watch(serverProvider(key).select((s) => s.data?.info.name)) ??
        server.name;
    final prefs = ref.watch(notificationPrefsProvider);
    final muted = prefs.serverMuted(key);
    final unread = ref.watch(
      activityProvider(key).select(
        (activity) => activity.channels.entries.any(
          (entry) => entry.value.unread && !prefs.muted(key, entry.key),
        ),
      ),
    );
    final mentions = ref.watch(activityProvider(key).select((a) => a.mentions));
    return Tooltip(
      message: '$name\n${_stateLabel(status)}',
      excludeFromSemantics: true,
      child: Hoverable(
        onTap: () => ref.read(navigationProvider.notifier).openServer(key),
        onSecondaryTap: (position) =>
            _showMenu(context, ref, position, name, muted),
        semanticLabel: '$name, ${_stateLabel(status)}',
        selected: selected,
        focusRadius: BorderRadius.circular(OcRadius.serverIconActive),
        builder: (context, state) => RailItemView(
          id: key,
          name: name,
          selected: selected,
          hovered: state.active,
          unread: unread && !muted,
          mentions: muted ? 0 : mentions,
          connection: switch (status.phase) {
            ConnectionPhase.connected => RailConnection.connected,
            ConnectionPhase.connecting ||
            ConnectionPhase.reconnecting => RailConnection.connecting,
            ConnectionPhase.failed => RailConnection.failed,
          },
        ),
      ),
    );
  }

  /// §4.1: Mark as read · Mute / Unmute notifications · Invite people ·
  /// Server settings (if permitted) · Copy address · — · Leave server.
  Future<void> _showMenu(
    BuildContext context,
    WidgetRef ref,
    Offset position,
    String name,
    bool muted,
  ) {
    final data = ref.read(serverProvider(server.key)).data;
    return showOcMenu(
      context: context,
      position: position,
      entries: [
        OcMenuItem(
          label: 'Mark as read',
          icon: OcIcons.markChatRead,
          onSelected: () =>
              ref.read(activityProvider(server.key).notifier).markAllRead(),
        ),
        OcMenuItem(
          label: muted ? 'Unmute notifications' : 'Mute notifications',
          icon: muted ? OcIcons.notifications : OcIcons.notificationsOff,
          onSelected: () => ref
              .read(notificationPrefsProvider.notifier)
              .toggleServer(server.key),
        ),
        if (data?.can(Permissions.createInvite) ?? false)
          OcMenuItem(
            label: 'Invite people',
            icon: OcIcons.personAdd,
            onSelected: () => showInvitePeople(context, serverKey: server.key),
          ),
        if (canOpenServerSettings(data))
          OcMenuItem(
            label: 'Server settings',
            icon: OcIcons.settings,
            onSelected: () =>
                showServerSettings(context, ref, serverKey: server.key),
          ),
        OcMenuItem(
          label: 'Copy address',
          icon: OcIcons.link,
          onSelected: () {
            Clipboard.setData(ClipboardData(text: server.key));
            showOcToast(context, 'Address copied');
          },
        ),
        const OcMenuDivider(),
        OcMenuItem(
          label: 'Leave server',
          icon: OcIcons.logout,
          onSelected: () => confirmLeaveServer(context, ref, server.key, name),
        ),
      ],
    );
  }
}

/// Asks before forgetting a server (§4.1). Phase 1 has no "leave" request,
/// so the membership stays on the server and adding it again works.
Future<void> confirmLeaveServer(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  String name,
) async {
  final leave = await confirmAction(
    context,
    title: 'Leave $name?',
    message:
        '$name will be removed from this device. You stay a member, so you '
        'can add it again later.',
    action: 'Leave server',
  );
  if (leave) await ref.read(repositoryProvider).removeServer(serverKey);
}

/// One rail item (§4.1), separate from its data so every state can be
/// drawn on its own.
class RailItemView extends StatelessWidget {
  const RailItemView({
    super.key,
    required this.id,
    required this.name,
    this.selected = false,
    this.hovered = false,
    this.unread = false,
    this.mentions = 0,
    this.connection = RailConnection.connected,
  });

  final Object id;
  final String name;
  final bool selected;
  final bool hovered;
  final bool unread;
  final int mentions;
  final RailConnection connection;

  static const double height = 76;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final pill = selected
        ? 34.0
        : hovered
        ? 18.0
        : unread
        ? 8.0
        : 0.0;
    return SizedBox(
      width: OcSize.railWidth,
      height: height,
      child: Stack(
        children: [
          Positioned(
            left: 0,
            top: 4 + (OcSize.serverIcon - pill) / 2,
            child: AnimatedContainer(
              duration: motion.morph,
              curve: OcMotion.curve,
              width: 4,
              height: pill,
              decoration: BoxDecoration(
                color: colors.text,
                borderRadius: const BorderRadius.horizontal(
                  right: Radius.circular(2),
                ),
              ),
            ),
          ),
          Positioned(
            top: 4,
            left: (OcSize.railWidth - OcSize.serverIcon) / 2,
            child: SizedBox.square(
              dimension: OcSize.serverIcon,
              child: Stack(
                clipBehavior: Clip.none,
                children: [
                  TweenAnimationBuilder<double>(
                    tween: Tween(
                      end: selected || hovered
                          ? OcRadius.serverIconActive
                          : OcRadius.serverIcon,
                    ),
                    duration: motion.morph,
                    curve: OcMotion.curve,
                    builder: (context, radius, _) => AnimatedContainer(
                      duration: motion.hover,
                      width: OcSize.serverIcon,
                      height: OcSize.serverIcon,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: selected
                            ? colors.accent
                            : hovered
                            ? colors.selected
                            : colors.hover,
                        borderRadius: BorderRadius.circular(radius),
                      ),
                      child: Text(
                        initialsOf(name),
                        textScaler: TextScaler.noScaling,
                        style: OcText.bodyStrong.copyWith(
                          fontSize: 16,
                          height: 1,
                          color: selected ? colors.onAccent : colors.text,
                        ),
                      ),
                    ),
                  ),
                  if (mentions > 0)
                    Positioned(
                      top: -4,
                      right: -6,
                      child: _Ringed(
                        ring: colors.rail,
                        radius: 12,
                        child: UnreadBadge(count: mentions),
                      ),
                    ),
                  if (connection != RailConnection.connected)
                    Positioned(
                      right: -4,
                      bottom: -4,
                      child: _Ringed(
                        ring: colors.rail,
                        radius: 10,
                        child: Container(
                          width: 18,
                          height: 18,
                          decoration: BoxDecoration(
                            color: colors.selected,
                            shape: BoxShape.circle,
                          ),
                          child: Icon(
                            connection == RailConnection.failed
                                ? OcIcons.error
                                : OcIcons.sync,
                            size: 12,
                            fill: 0,
                            color: colors.text,
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
          Positioned(
            left: 4,
            right: 4,
            top: 4 + OcSize.serverIcon + 4,
            child: Text(
              name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              textAlign: TextAlign.center,
              style: OcText.meta.copyWith(
                color: selected ? colors.text : colors.textSecondary,
                fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// A 2 px ring in the rail's color around a badge (§4.1).
class _Ringed extends StatelessWidget {
  const _Ringed({
    required this.ring,
    required this.radius,
    required this.child,
  });

  final Color ring;
  final double radius;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(
        color: ring,
        borderRadius: BorderRadius.circular(radius),
      ),
      child: child,
    );
  }
}
