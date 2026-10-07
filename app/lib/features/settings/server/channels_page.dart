import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/dialogs/create_channel_dialog.dart';
import 'package:opencord/features/settings/server/channel_editor.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/toast.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';

/// Channels (§8.2): every category and channel, reordered by dragging
/// (channels within their group, categories as blocks); the chosen one's
/// Overview and Permissions on the right.
class ServerChannelsPage extends ConsumerStatefulWidget {
  const ServerChannelsPage({
    super.key,
    required this.serverKey,
    this.initialChannel,
  });

  final String serverKey;
  final int? initialChannel;

  @override
  ConsumerState<ServerChannelsPage> createState() => _ServerChannelsPageState();
}

class _ServerChannelsPageState extends ConsumerState<ServerChannelsPage> {
  late int? _selected = widget.initialChannel;

  /// Saves [order] as the positions of one group of channels.
  Future<void> _reorder(List<Channel> order) async {
    try {
      await ref.read(repositoryProvider).reorderChannels(widget.serverKey, {
        for (final (index, channel) in order.indexed) channel.id: index,
      });
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
  }

  Widget _segment(List<Channel> channels, Channel? selected) {
    if (channels.isEmpty) return const SizedBox.shrink();
    return ReorderableListView(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      buildDefaultDragHandles: false,
      proxyDecorator: (child, index, animation) =>
          Material(type: MaterialType.transparency, child: child),
      onReorderItem: (from, to) {
        final order = [...channels];
        final moved = order.removeAt(from);
        order.insert(to.clamp(0, order.length), moved);
        _reorder(order);
      },
      children: [
        for (final (index, channel) in channels.indexed)
          ReorderableDragStartListener(
            key: ValueKey(channel.id),
            index: index,
            child: _ChannelTile(
              channel: channel,
              selected: channel.id == selected?.id,
              onTap: () => setState(() => _selected = channel.id),
            ),
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final channels = ref.watch(
      serverProvider(widget.serverKey).select((state) => state.data?.channels),
    );
    if (channels == null) return const SizedBox.shrink();
    final groups = channelTree(channels.values);
    final loose = groups.where((group) => group.category == null).firstOrNull;
    final categories = [
      for (final group in groups)
        if (group.category case final category?) (category, group.channels),
    ];
    final selected =
        channels[_selected] ??
        groups
            .expand((group) => [?group.category, ...group.channels])
            .firstOrNull;
    List<Widget> segments(List<Channel> group) => [
      _segment([
        for (final channel in group)
          if (channel.kind.isTextLike) channel,
      ], selected),
      _segment([
        for (final channel in group)
          if (channel.kind == ChannelKind.voice) channel,
      ], selected),
    ];
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 220,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              OcButton(
                label: 'Create channel',
                icon: OcIcons.add,
                onPressed: () =>
                    showCreateChannel(context, serverKey: widget.serverKey),
              ),
              const SizedBox(height: OcSpace.s6),
              OcButton(
                label: 'Create category',
                icon: OcIcons.add,
                onPressed: () =>
                    showCreateCategory(context, serverKey: widget.serverKey),
              ),
              const SizedBox(height: OcSpace.s12),
              if (loose != null) ...segments(loose.channels),
              // Categories move as a block, their channels with them.
              ReorderableListView(
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                buildDefaultDragHandles: false,
                proxyDecorator: (child, index, animation) =>
                    Material(type: MaterialType.transparency, child: child),
                onReorderItem: (from, to) {
                  final order = [
                    for (final (category, _) in categories) category,
                  ];
                  final moved = order.removeAt(from);
                  order.insert(to.clamp(0, order.length), moved);
                  _reorder(order);
                },
                children: [
                  for (final (index, (category, children))
                      in categories.indexed)
                    Column(
                      key: ValueKey(category.id),
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Padding(
                          padding: const EdgeInsets.only(top: OcSpace.s8),
                          child: ReorderableDragStartListener(
                            index: index,
                            child: _ChannelTile(
                              channel: category,
                              selected: category.id == selected?.id,
                              onTap: () =>
                                  setState(() => _selected = category.id),
                            ),
                          ),
                        ),
                        ...segments(children),
                      ],
                    ),
                ],
              ),
              if (groups.isEmpty)
                Text(
                  'No channels yet.',
                  style: OcText.small.copyWith(color: colors.textMuted),
                ),
            ],
          ),
        ),
        const SizedBox(width: OcSpace.s24),
        Expanded(
          child: selected == null
              ? const SizedBox.shrink()
              : ChannelEditor(
                  key: ValueKey(selected.id),
                  serverKey: widget.serverKey,
                  channel: selected,
                  onDeleted: () {
                    if (mounted) setState(() => _selected = null);
                  },
                ),
        ),
      ],
    );
  }
}

class _ChannelTile extends StatelessWidget {
  const _ChannelTile({
    required this.channel,
    required this.selected,
    required this.onTap,
  });

  final Channel channel;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final category = channel.isCategory;
    return Hoverable(
      onTap: onTap,
      semanticLabel: category ? '${channel.name} category' : channel.name,
      selected: selected,
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: 34,
        margin: const EdgeInsets.only(bottom: 2),
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
        decoration: BoxDecoration(
          color: selected
              ? colors.selected
              : state.active
              ? colors.hover
              : null,
          borderRadius: BorderRadius.circular(OcRadius.row),
        ),
        child: Row(
          children: [
            Icon(
              switch (channel.kind) {
                ChannelKind.category => OcIcons.expandMore,
                ChannelKind.voice => OcIcons.volumeUp,
                ChannelKind.announcement => OcIcons.campaign,
                ChannelKind.text => OcIcons.tag,
              },
              size: 16,
              color: colors.textSecondary,
            ),
            const SizedBox(width: OcSpace.s8),
            Expanded(
              child: EllipsisText(
                category ? channel.name.toUpperCase() : channel.name,
                style: (category ? OcText.label : OcText.body).copyWith(
                  fontWeight: selected ? FontWeight.w600 : null,
                  color: category ? colors.textMuted : colors.text,
                ),
              ),
            ),
            Icon(OcIcons.dragIndicator, size: 14, color: colors.textMuted),
          ],
        ),
      ),
    );
  }
}
