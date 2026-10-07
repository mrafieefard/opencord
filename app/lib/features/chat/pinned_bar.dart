import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// One line of plain text for a pinned message.
String _preview(WidgetRef ref, ChannelRef channel, Message message) {
  final data = ref.read(serverProvider(channel.server)).data;
  return previewText(
    message.content,
    user: (id) => data?.members[id]?.displayName,
    channel: (id) => data?.channels[id]?.name,
  );
}

bool _canUnpin(WidgetRef ref, ChannelRef channel) =>
    ref
        .watch(serverProvider(channel.server).select((state) => state.data))
        ?.permissionsIn(channel.channel)
        .has(Permissions.manageMessages) ??
    false;

/// Unpins with an Undo toast instead of a confirm dialog (§16).
Future<void> _unpin(
  BuildContext context,
  WidgetRef ref,
  ChannelRef channel,
  Message message,
) async {
  final repository = ref.read(repositoryProvider);
  try {
    await repository.setPinned(
      channel.server,
      channel.channel,
      message.id,
      pinned: false,
    );
    if (!context.mounted) return;
    showOcToast(
      context,
      'Message unpinned',
      actionLabel: 'Undo',
      onAction: () => repository.setPinned(
        channel.server,
        channel.channel,
        message.id,
        pinned: true,
      ),
    );
  } on RepoException catch (error) {
    if (context.mounted) showOcToast(context, error.message);
  }
}

/// The bar under the header (§4.4, Telegram): the newest pin first; a click
/// jumps to it and moves on to the next older one.
class PinnedBar extends ConsumerStatefulWidget {
  const PinnedBar({super.key, required this.channel, required this.onJumpTo});

  final ChannelRef channel;
  final ValueChanged<int> onJumpTo;

  @override
  ConsumerState<PinnedBar> createState() => _PinnedBarState();
}

class _PinnedBarState extends ConsumerState<PinnedBar> {
  int _index = 0;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final pins = ref.watch(pinsProvider(widget.channel));
    if (pins.isEmpty) return const SizedBox.shrink();
    final index = _index % pins.length;
    final pin = pins[index];
    final label = pins.length == 1
        ? 'Pinned message'
        : 'Pinned message ${pins.length - index} of ${pins.length}';
    final preview = _preview(ref, widget.channel, pin);
    return Container(
      constraints: const BoxConstraints(minHeight: OcSize.pinnedBar),
      decoration: BoxDecoration(
        color: colors.sidebar,
        border: Border(bottom: BorderSide(color: colors.border)),
      ),
      child: Hoverable(
        onTap: () {
          widget.onJumpTo(pin.id);
          setState(() => _index = (index + 1) % pins.length);
        },
        semanticLabel: '$label: $preview',
        builder: (context, state) => AnimatedContainer(
          duration: OcMotion.of(context).hover,
          color: state.active ? colors.hover : null,
          constraints: const BoxConstraints(minHeight: OcSize.pinnedBar - 1),
          padding: const EdgeInsets.fromLTRB(
            OcSpace.s16,
            OcSpace.s4,
            OcSpace.s8,
            OcSpace.s4,
          ),
          child: Row(
            children: [
              Container(width: 2, height: 32, color: colors.text),
              const SizedBox(width: OcSpace.s10),
              Expanded(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      label,
                      maxLines: 1,
                      style: OcText.small.copyWith(
                        fontWeight: FontWeight.w600,
                        color: colors.text,
                      ),
                    ),
                    Text(
                      preview,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.small.copyWith(color: colors.textSecondary),
                    ),
                  ],
                ),
              ),
              if (_canUnpin(ref, widget.channel))
                OcIconButton(
                  icon: OcIcons.close,
                  tooltip: 'Unpin',
                  size: OcIconButtonSize.compact,
                  onPressed: () => _unpin(context, ref, widget.channel, pin),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Every pinned message of a channel, from the header's pin button.
class PinnedList extends ConsumerWidget {
  const PinnedList({super.key, required this.channel, required this.onJumpTo});

  final ChannelRef channel;
  final ValueChanged<int> onJumpTo;

  static const double width = 360;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final pins = ref.watch(pinsProvider(channel));
    final members = ref.watch(
      serverProvider(channel.server).select((state) => state.data?.members),
    );
    final canUnpin = _canUnpin(ref, channel);
    return SizedBox(
      width: width,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              OcSpace.s8,
              OcSpace.s4,
              OcSpace.s8,
              OcSpace.s8,
            ),
            child: Text(
              'Pinned messages',
              style: OcText.header.copyWith(color: colors.text),
            ),
          ),
          if (pins.isEmpty)
            Padding(
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s8,
                0,
                OcSpace.s8,
                OcSpace.s8,
              ),
              child: Text(
                'Nothing is pinned here yet. Pin a message from its menu to keep it at hand.',
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            )
          else
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 420),
              child: ListView(
                shrinkWrap: true,
                children: [
                  for (final pin in pins)
                    Hoverable(
                      onTap: () => onJumpTo(pin.id),
                      semanticLabel:
                          'Pinned message from ${members?[pin.authorId]?.displayName ?? 'someone'}',
                      builder: (context, state) => AnimatedContainer(
                        duration: OcMotion.of(context).hover,
                        padding: const EdgeInsets.all(OcSpace.s8),
                        decoration: BoxDecoration(
                          color: state.active ? colors.hover : null,
                          borderRadius: BorderRadius.circular(OcRadius.row),
                        ),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                    '${members?[pin.authorId]?.displayName ?? 'Unknown user'} · ${rowTime(pin.createdAt, ref.read(clockProvider)())}',
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: OcText.small.copyWith(
                                      fontWeight: FontWeight.w600,
                                      color: colors.text,
                                    ),
                                  ),
                                  const SizedBox(height: OcSpace.s2),
                                  Text(
                                    _preview(ref, channel, pin),
                                    maxLines: 3,
                                    overflow: TextOverflow.ellipsis,
                                    style: OcText.small.copyWith(
                                      color: colors.textSecondary,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                            if (canUnpin)
                              OcIconButton(
                                icon: OcIcons.close,
                                tooltip: 'Unpin',
                                size: OcIconButtonSize.compact,
                                onPressed: () =>
                                    _unpin(context, ref, channel, pin),
                              ),
                          ],
                        ),
                      ),
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}
