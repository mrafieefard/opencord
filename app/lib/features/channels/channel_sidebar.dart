import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/channels/voice_channel_row.dart';
import 'package:opencord/features/dialogs/create_channel_dialog.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/switcher/quick_switcher.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

export 'server_head.dart';

/// The channel sidebar (§4.2).
class ChannelSidebar extends ConsumerWidget {
  const ChannelSidebar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final server = ref.watch(currentServerProvider);
    return ColoredBox(
      color: colors.sidebar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SearchLauncher(),
          Expanded(
            child: server == null
                ? const SizedBox()
                : _ChannelList(serverKey: server),
          ),
        ],
      ),
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
      padding: const EdgeInsets.all(8),
      child: Hoverable(
        onTap: () => showQuickSwitcher(context, ref),
        semanticLabel: 'Search, jump to a channel, server or member',
        focusRadius: BorderRadius.circular(18),
        builder: (context, state) => AnimatedContainer(
          key: ShellKeys.search,
          duration: OcMotion.of(context).hover,
          height: 36,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(
            color: state.active ? colors.selected : colors.hover,
            borderRadius: BorderRadius.circular(18),
          ),
          child: Row(
            children: [
              Icon(OcIcons.search, size: 18, color: colors.textMuted),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  'Search',
                  style: OcText.body.copyWith(
                    fontSize: 14,
                    color: colors.textMuted,
                  ),
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

/// What a right-click on the channel list offers, as in Discord: create a
/// text or voice channel, in [category] when on one, or a category, and
/// invite people. Empty for people who may do none of it.
List<OcMenuEntry> _createEntries(
  BuildContext context,
  WidgetRef ref,
  String serverKey, {
  Channel? category,
}) {
  final data = ref.read(serverProvider(serverKey)).data;
  final manage = data?.can(Permissions.manageChannels) ?? false;
  final invite = data?.can(Permissions.createInvite) ?? false;
  return [
    if (manage) ...[
      OcMenuItem(
        label: 'Create text channel',
        icon: OcIcons.tag,
        onSelected: () => showCreateChannel(
          context,
          serverKey: serverKey,
          parentId: category?.id,
        ),
      ),
      OcMenuItem(
        label: 'Create voice channel',
        icon: OcIcons.volumeUp,
        onSelected: () => showCreateChannel(
          context,
          serverKey: serverKey,
          parentId: category?.id,
          kind: ChannelKind.voice,
        ),
      ),
      OcMenuItem(
        label: 'Create category',
        icon: OcIcons.createNewFolder,
        onSelected: () => showCreateCategory(context, serverKey: serverKey),
      ),
    ],
    if (manage && invite) const OcMenuDivider(),
    if (invite)
      OcMenuItem(
        label: 'Invite people',
        icon: OcIcons.personAdd,
        onSelected: () => showInvitePeople(context, serverKey: serverKey),
      ),
  ];
}

Future<void> _showCreateMenu(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  Offset position, {
  Channel? category,
}) async {
  final entries = _createEntries(context, ref, serverKey, category: category);
  if (entries.isEmpty) return;
  await showOcMenu(context: context, position: position, entries: entries);
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

    Widget row(Channel channel, {int? reorderIndex}) =>
        channel.kind == ChannelKind.voice
        ? VoiceChannelRow(
            serverKey: serverKey,
            channel: channel,
            reorderIndex: reorderIndex,
          )
        : ChannelRow(serverKey: serverKey, channel: channel);

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
            // A voice channel drags by its own line, so the people in it
            // can be dragged to other channels.
            if (channel.kind == ChannelKind.voice)
              KeyedSubtree(
                key: ValueKey(channel.id),
                child: row(channel, reorderIndex: index),
              )
            else
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
            if (channel.kind.isTextLike || channel.kind == ChannelKind.plugin)
              channel,
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
    // Categories drag as blocks, their channels with them (§16). A
    // right-click between channels offers to create one.
    final list = ReorderableListView(
      padding: const EdgeInsets.only(top: 0, bottom: 8),
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
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onSecondaryTapUp: (details) =>
          _showCreateMenu(context, ref, serverKey, details.globalPosition),
      child: list,
    );
  }
}

class _CategoryRow extends ConsumerStatefulWidget {
  const _CategoryRow({
    required this.serverKey,
    required this.category,
    required this.collapsed,
    this.dragIndex,
  });

  final String serverKey;
  final Channel category;
  final bool collapsed;
  final int? dragIndex;

  @override
  ConsumerState<_CategoryRow> createState() => _CategoryRowState();
}

class _CategoryRowState extends ConsumerState<_CategoryRow> {
  bool _hovered = false;
  bool _plusFocused = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final manage =
        ref.watch(
          serverProvider(
            widget.serverKey,
          ).select((s) => s.data?.can(Permissions.manageChannels)),
        ) ??
        false;

    final showPlus = _hovered || _plusFocused;

    final toggle = Hoverable(
      onTap: () => ref
          .read(collapsedCategoriesProvider.notifier)
          .toggle(widget.serverKey, widget.category.id),
      onSecondaryTap: (position) => _showCreateMenu(
        context,
        ref,
        widget.serverKey,
        position,
        category: widget.category,
      ),
      semanticLabel:
          '${widget.category.name}, ${widget.collapsed ? 'collapsed' : 'expanded'}',
      cursor: SystemMouseCursors.basic,
      builder: (context, state) => Row(
        children: [
          AnimatedRotation(
            turns: widget.collapsed ? -0.25 : 0,
            duration: OcMotion.of(context).morph,
            curve: OcMotion.curve,
            child: Icon(
              OcIcons.expandMore,
              size: 16,
              color: state.active ? colors.textSecondary : colors.textMuted,
            ),
          ),
          const SizedBox(width: 4),
          Expanded(
            child: Text(
              widget.category.name.toUpperCase(),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: OcText.meta.copyWith(
                fontSize: 11.5,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.7,
                color: colors.textMuted,
              ),
            ),
          ),
        ],
      ),
    );

    final draggableToggle = widget.dragIndex == null
        ? toggle
        : ReorderableDragStartListener(index: widget.dragIndex!, child: toggle);

    return MouseRegion(
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(10, 8, 12, 4),
        child: Row(
          children: [
            Expanded(child: draggableToggle),
            if (manage)
              Focus(
                onFocusChange: (f) => setState(() => _plusFocused = f),
                child: AnimatedOpacity(
                  opacity: showPlus ? 1.0 : 0.0,
                  duration: OcMotion.of(context).hover,
                  curve: OcMotion.curve,
                  child: IgnorePointer(
                    ignoring: !showPlus,
                    child: SizedBox(
                      width: 20,
                      height: 20,
                      child: Hoverable(
                        onTap: () => showCreateChannel(
                          context,
                          serverKey: widget.serverKey,
                          parentId: widget.category.id,
                        ),
                        focusRadius: BorderRadius.circular(4),
                        semanticLabel:
                            'Create channel in ${widget.category.name}',
                        builder: (context, state) => Center(
                          child: Icon(
                            OcIcons.add,
                            size: 16,
                            color: colors.textMuted,
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
