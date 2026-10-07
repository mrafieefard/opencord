import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/channels/user_panel.dart';
import 'package:opencord/features/channels/voice_channel_row.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/servers/server_rail.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/switcher/quick_switcher.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The channel sidebar (§4.2).
class ChannelSidebar extends ConsumerWidget {
  const ChannelSidebar({super.key, this.leadingControls = false});

  /// Carries left-hand window controls (§3.1).
  final bool leadingControls;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final server = ref.watch(currentServerProvider);
    return ColoredBox(
      color: colors.sidebar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          HeaderBar(
            leadingControls: leadingControls,
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
            child: server == null
                ? const SizedBox()
                : _ServerHeader(serverKey: server),
          ),
          const _SearchLauncher(),
          Expanded(
            child: server == null
                ? const SizedBox()
                : _ChannelList(serverKey: server),
          ),
          const VoiceConnectedPanel(),
          const UserPanel(),
        ],
      ),
    );
  }
}

class _ServerHeader extends ConsumerWidget {
  const _ServerHeader({required this.serverKey});

  final String serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final state = ref.watch(serverProvider(serverKey));
    final online = ref.watch(
      presenceProvider(serverKey).select(
        (p) => p.presences.values
            .where((presence) => presence != Presence.offline)
            .length,
      ),
    );
    final name =
        state.data?.info.name ??
        ref
            .watch(serverListProvider)
            .where((s) => s.key == serverKey)
            .firstOrNull
            ?.name ??
        serverKey;
    final members = state.data?.members.length ?? 0;
    final status = switch (state.connection.phase) {
      ConnectionPhase.connected =>
        '${countLabel(online)} online · ${countLabel(members)} members',
      ConnectionPhase.connecting => 'Connecting…',
      ConnectionPhase.reconnecting => 'Reconnecting…',
      ConnectionPhase.failed => 'Connection failed',
    };
    return Hoverable(
      onTap: () => _showMenu(context, ref, name),
      semanticLabel: '$name, $status. Server menu',
      builder: (context, hover) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        constraints: const BoxConstraints(minHeight: 44),
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
        decoration: BoxDecoration(
          color: hover.active ? colors.hover : null,
          borderRadius: BorderRadius.circular(OcRadius.row),
        ),
        child: Row(
          children: [
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.header.copyWith(color: colors.text),
                  ),
                  Row(
                    children: [
                      ConnectionDot(
                        connected: state.connection.isConnected,
                        size: 6,
                      ),
                      const SizedBox(width: OcSpace.s6),
                      Expanded(
                        child: Text(
                          status,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: OcText.meta.copyWith(color: colors.textMuted),
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
            Icon(
              OcIcons.expandMore,
              size: OcSize.iconRow,
              color: colors.textSecondary,
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _showMenu(BuildContext context, WidgetRef ref, String name) {
    final muted = ref.read(notificationPrefsProvider).serverMuted(serverKey);
    final rect = globalRectOf(context);
    return showOcMenu(
      context: context,
      position: rect.bottomLeft.translate(0, 6),
      entries: [
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
  }
}

/// Opens the quick switcher (§4.2, §4.9).
class _SearchLauncher extends ConsumerWidget {
  const _SearchLauncher();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        OcSpace.s12,
        OcSpace.s10,
        OcSpace.s12,
        OcSpace.s4,
      ),
      child: Hoverable(
        onTap: () => showQuickSwitcher(context, ref),
        semanticLabel: 'Search, jump to a channel, server or member',
        focusRadius: BorderRadius.circular(OcRadius.searchPill),
        builder: (context, state) => AnimatedContainer(
          duration: OcMotion.of(context).hover,
          height: 36,
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s12),
          decoration: BoxDecoration(
            color: state.active ? colors.selected : colors.hover,
            borderRadius: BorderRadius.circular(OcRadius.searchPill),
          ),
          child: Row(
            children: [
              Icon(
                OcIcons.search,
                size: OcSize.iconRow,
                color: colors.textMuted,
              ),
              const SizedBox(width: OcSpace.s8),
              Expanded(
                child: Text(
                  'Search',
                  style: OcText.body.copyWith(color: colors.textMuted),
                ),
              ),
              KeyHint(
                appShortcut(
                  LogicalKeyboardKey.keyK,
                  Theme.of(context).platform,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ChannelList extends ConsumerWidget {
  const _ChannelList({required this.serverKey});

  final String serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final channels = ref.watch(
      serverProvider(serverKey).select((s) => s.data?.channels),
    );
    if (channels == null) return const SizedBox();
    final collapsed = ref.watch(collapsedCategoriesProvider);
    final current = ref.watch(currentChannelProvider);
    final voice = ref.watch(voiceSessionProvider);
    final inVoiceHere = voice.serverKey == serverKey ? voice.channelId : null;
    final items = <Widget>[];
    for (final group in channelTree(channels.values)) {
      final category = group.category;
      final folded =
          category != null &&
          collapsed.contains(categoryKey(serverKey, category.id));
      if (category != null) {
        items.add(
          _CategoryRow(
            serverKey: serverKey,
            category: category,
            collapsed: folded,
          ),
        );
      }
      for (final channel in group.channels) {
        // Collapsed categories still show the open channel and the voice
        // channel the user is in (§4.2).
        if (folded && channel.id != current && channel.id != inVoiceHere) {
          continue;
        }
        items.add(
          channel.kind == ChannelKind.voice
              ? VoiceChannelRow(
                  key: ValueKey(channel.id),
                  serverKey: serverKey,
                  channel: channel,
                )
              : Padding(
                  key: ValueKey(channel.id),
                  padding: const EdgeInsets.only(bottom: 2),
                  child: ChannelRow(serverKey: serverKey, channel: channel),
                ),
        );
      }
    }
    return ListView.builder(
      padding: const EdgeInsets.only(top: OcSpace.s4, bottom: OcSpace.s12),
      itemCount: items.length,
      itemBuilder: (context, index) => items[index],
    );
  }
}

class _CategoryRow extends ConsumerWidget {
  const _CategoryRow({
    required this.serverKey,
    required this.category,
    required this.collapsed,
  });

  final String serverKey;
  final Channel category;
  final bool collapsed;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return Hoverable(
      onTap: () => ref
          .read(collapsedCategoriesProvider.notifier)
          .toggle(serverKey, category.id),
      semanticLabel:
          '${category.name}, ${collapsed ? 'collapsed' : 'expanded'}',
      cursor: SystemMouseCursors.basic,
      builder: (context, state) => Padding(
        padding: const EdgeInsets.fromLTRB(
          OcSpace.s12,
          OcSpace.s12,
          OcSpace.s12,
          OcSpace.s4,
        ),
        child: Row(
          children: [
            AnimatedRotation(
              turns: collapsed ? -0.25 : 0,
              duration: OcMotion.of(context).morph,
              child: Icon(
                OcIcons.expandMore,
                size: OcSize.iconInline,
                color: state.active ? colors.textSecondary : colors.textMuted,
              ),
            ),
            const SizedBox(width: 2),
            Expanded(child: SectionLabel(category.name)),
          ],
        ),
      ),
    );
  }
}
