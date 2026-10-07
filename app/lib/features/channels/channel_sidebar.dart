import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/channels/user_panel.dart';
import 'package:opencord/features/channels/voice_channel_row.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/dialogs/create_channel_dialog.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/settings/server_settings.dart';
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
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';
import 'package:opencord/core/repository/repository.dart';

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
    final data = ref.read(serverProvider(serverKey)).data;
    final invite = data?.can(Permissions.createInvite) ?? false;
    final manage = data?.can(Permissions.manageChannels) ?? false;
    final rect = globalRectOf(context);
    return showOcMenu(
      context: context,
      position: rect.bottomLeft.translate(0, 6),
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
            onSelected: () => showCreateChannel(context, serverKey: serverKey),
          ),
          OcMenuItem(
            label: 'Create category',
            icon: OcIcons.createNewFolder,
            onSelected: () => showCreateCategory(context, serverKey: serverKey),
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

  /// Saves [order] as the positions of one group (§16 drag and drop).
  Future<void> _reorder(
    BuildContext context,
    WidgetRef ref,
    List<Channel> order,
  ) async {
    try {
      await ref.read(repositoryProvider).reorderChannels(serverKey, {
        for (final (index, channel) in order.indexed) channel.id: index,
      });
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, error.message);
    }
  }

  static Widget _lift(Widget child, int index, Animation<double> animation) =>
      Material(type: MaterialType.transparency, child: child);

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final channels = ref.watch(
      serverProvider(serverKey).select((s) => s.data?.channels),
    );
    if (channels == null) return const SizedBox();
    final manage =
        ref.watch(
          serverProvider(
            serverKey,
          ).select((s) => s.data?.can(Permissions.manageChannels)),
        ) ??
        false;
    final collapsed = ref.watch(collapsedCategoriesProvider);
    final current = ref.watch(currentChannelProvider);
    final voice = ref.watch(voiceSessionProvider);
    final inVoiceHere = voice.serverKey == serverKey ? voice.channelId : null;

    Widget row(Channel channel) => channel.kind == ChannelKind.voice
        ? VoiceChannelRow(serverKey: serverKey, channel: channel)
        : Padding(
            padding: const EdgeInsets.only(bottom: 2),
            child: ChannelRow(serverKey: serverKey, channel: channel),
          );

    /// Text or voice channels of one group, which drag among themselves.
    Widget segment(List<Channel> group, {required bool draggable}) {
      if (group.isEmpty) return const SizedBox.shrink();
      if (!draggable) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            for (final channel in group)
              KeyedSubtree(key: ValueKey(channel.id), child: row(channel)),
          ],
        );
      }
      return ReorderableListView(
        shrinkWrap: true,
        physics: const NeverScrollableScrollPhysics(),
        buildDefaultDragHandles: false,
        proxyDecorator: _lift,
        onReorderItem: (from, to) {
          final order = [...group];
          final moved = order.removeAt(from);
          order.insert(to.clamp(0, order.length), moved);
          _reorder(context, ref, order);
        },
        children: [
          for (final (index, channel) in group.indexed)
            ReorderableDragStartListener(
              key: ValueKey(channel.id),
              index: index,
              child: row(channel),
            ),
        ],
      );
    }

    List<Widget> rows(ChannelGroup group, {required bool folded}) {
      // Collapsed categories still show the open channel and the voice
      // channel the user is in (§4.2); they do not reorder.
      final shown = [
        for (final channel in group.channels)
          if (!folded || channel.id == current || channel.id == inVoiceHere)
            channel,
      ];
      final draggable = manage && !folded;
      return [
        segment([
          for (final channel in shown)
            if (channel.kind.isTextLike) channel,
        ], draggable: draggable),
        segment([
          for (final channel in shown)
            if (channel.kind == ChannelKind.voice) channel,
        ], draggable: draggable),
      ];
    }

    final groups = channelTree(channels.values);
    final loose = groups.where((group) => group.category == null).firstOrNull;
    final categories = [
      for (final group in groups)
        if (group.category case final category?) (category, group),
    ];
    // Categories drag as blocks, their channels with them (§16).
    return ReorderableListView(
      padding: const EdgeInsets.only(top: OcSpace.s4, bottom: OcSpace.s12),
      buildDefaultDragHandles: false,
      proxyDecorator: _lift,
      header: loose == null
          ? null
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: rows(loose, folded: false),
            ),
      onReorderItem: (from, to) {
        final order = [for (final (category, _) in categories) category];
        final moved = order.removeAt(from);
        order.insert(to.clamp(0, order.length), moved);
        _reorder(context, ref, order);
      },
      children: [
        for (final (index, (category, group)) in categories.indexed)
          Column(
            key: ValueKey(category.id),
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _CategoryRow(
                serverKey: serverKey,
                category: category,
                collapsed: collapsed.contains(
                  categoryKey(serverKey, category.id),
                ),
                dragIndex: manage ? index : null,
              ),
              ...rows(
                group,
                folded: collapsed.contains(categoryKey(serverKey, category.id)),
              ),
            ],
          ),
      ],
    );
  }
}

class _CategoryRow extends ConsumerWidget {
  const _CategoryRow({
    required this.serverKey,
    required this.category,
    required this.collapsed,
    this.dragIndex,
  });

  final String serverKey;
  final Channel category;
  final bool collapsed;

  /// The category's place among the draggable blocks, for people who may
  /// reorder them. Only the label drags, so the "+" stays a button.
  final int? dragIndex;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final manage =
        ref.watch(
          serverProvider(
            serverKey,
          ).select((s) => s.data?.can(Permissions.manageChannels)),
        ) ??
        false;
    final label = _toggle(context, ref);
    final toggle = dragIndex == null
        ? label
        : ReorderableDragStartListener(index: dragIndex!, child: label);
    if (!manage) return toggle;
    return Row(
      children: [
        Expanded(child: toggle),
        Padding(
          padding: const EdgeInsets.only(top: OcSpace.s8, right: OcSpace.s8),
          child: OcIconButton(
            icon: OcIcons.add,
            tooltip: 'Create channel in ${category.name}',
            size: OcIconButtonSize.compact,
            onPressed: () => showCreateChannel(
              context,
              serverKey: serverKey,
              parentId: category.id,
            ),
          ),
        ),
      ],
    );
  }

  Widget _toggle(BuildContext context, WidgetRef ref) {
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
