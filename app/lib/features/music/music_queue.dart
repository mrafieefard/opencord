import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/links.dart';
import 'package:opencord/features/music/music_access.dart';
import 'package:opencord/features/music/music_artwork.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

const _rowHeight = 52.0;

/// "12 tracks · 47 min".
String queueSummary(MusicSession session) {
  final count = session.queue.length;
  final minutes = session.queueLength.inMinutes;
  final tracks = count == 1 ? '1 track' : '$count tracks';
  return minutes == 0 ? tracks : '$tracks · $minutes min';
}

/// Where an item moved to [newIndex] (its place once it is out of the
/// list) lands, as the server takes it: before the item that follows it
/// there, or at the end.
String? beforeItemAfterMove(
  List<MusicQueueItem> queue,
  int oldIndex,
  int newIndex,
) {
  final rest = [...queue]..removeAt(oldIndex);
  return newIndex < rest.length ? rest[newIndex].id : null;
}

/// Moves a queued item before another one, or to the end: here at once,
/// then on the server, whose answer follows.
void moveQueueItem(
  BuildContext context,
  WidgetRef ref,
  MusicChannel channel,
  String itemId,
  String? beforeItemId,
) {
  final provider = musicSessionProvider(channel);
  final session = ref.read(provider).value;
  if (session == null) return;
  final queue = [...session.queue];
  final from = queue.indexWhere((each) => each.id == itemId);
  if (from < 0) return;
  final item = queue.removeAt(from);
  final at = beforeItemId == null
      ? queue.length
      : queue.indexWhere((each) => each.id == beforeItemId);
  queue.insert(at < 0 ? queue.length : at, item);
  ref
      .read(provider.notifier)
      .apply(
        MusicSession(
          channelId: session.channelId,
          version: session.version,
          state: session.state,
          nowPlaying: session.nowPlaying,
          queue: queue,
          history: session.history,
          position: session.position,
          anchor: session.anchor,
          repeat: session.repeat,
          shuffle: session.shuffle,
          waitingForNode: session.waitingForNode,
          pausedEmpty: session.pausedEmpty,
          skipVotes: session.skipVotes,
          skipVotesNeeded: session.skipVotesNeeded,
        ),
      );
  musicAction(
    context,
    ref,
    channel,
    (repository) => repository.musicMove(
      channel.server,
      channel.channel,
      itemId,
      beforeItemId: beforeItemId,
    ),
  );
}

/// Up next (§19.4–19.5): virtualized, reordered by dragging with Manage
/// the queue; every drag has a menu alternative. A change rebuilds only
/// the rows it touched (§0.1): a row whose item, place and permissions
/// are the same keeps its widget.
class MusicQueueList extends ConsumerStatefulWidget {
  const MusicQueueList({super.key, required this.channel});

  final MusicChannel channel;

  @override
  ConsumerState<MusicQueueList> createState() => _MusicQueueListState();
}

typedef _RowShape = (MusicQueueItem item, int index, bool reorder);

class _MusicQueueListState extends ConsumerState<MusicQueueList> {
  final _rows = <String, (_RowShape, Widget)>{};

  Widget _row(MusicQueueItem item, int index, bool reorder) {
    final shape = (item, index, reorder);
    final kept = _rows[item.id];
    if (kept != null && kept.$1 == shape) return kept.$2;
    final channel = widget.channel;
    final row = MusicQueueRow(
      key: ValueKey(item.id),
      channel: channel,
      item: item,
      index: index,
      dragHandle: reorder
          ? ReorderableDragStartListener(
              index: index,
              child: Padding(
                padding: const EdgeInsets.all(OcSpace.s4),
                child: Icon(
                  OcIcons.dragIndicator,
                  size: OcSize.iconRow,
                  color: context.oc.textMuted,
                ),
              ),
            )
          : null,
    );
    _rows[item.id] = (shape, row);
    return row;
  }

  @override
  Widget build(BuildContext context) {
    final channel = widget.channel;
    final queue = ref.watch(
      musicSessionProvider(
        channel,
      ).select((value) => value.value?.queue ?? const <MusicQueueItem>[]),
    );
    final reorder = ref.watch(
      musicAccessProvider(channel).select((access) => access.canManageQueue),
    );
    // Rows that left the queue are let go.
    final ids = {for (final item in queue) item.id};
    _rows.removeWhere((id, _) => !ids.contains(id));
    if (queue.isEmpty) {
      return const SliverToBoxAdapter(child: SizedBox.shrink());
    }
    return SliverReorderableList(
      itemCount: queue.length,
      itemExtentBuilder: (_, _) => _rowHeight,
      onReorderItem: (oldIndex, newIndex) {
        if (oldIndex == newIndex) return;
        moveQueueItem(
          context,
          ref,
          channel,
          queue[oldIndex].id,
          beforeItemAfterMove(queue, oldIndex, newIndex),
        );
      },
      proxyDecorator: (child, _, _) => Material(
        color: context.oc.elevated,
        borderRadius: BorderRadius.circular(8),
        child: child,
      ),
      itemBuilder: (context, index) => _row(queue[index], index, reorder),
    );
  }
}

/// A track in Up next: 40 px artwork, title, "artist · duration", who
/// added it, the drag handle and its menu.
class MusicQueueRow extends ConsumerWidget {
  const MusicQueueRow({
    super.key,
    required this.channel,
    required this.item,
    required this.index,
    this.dragHandle,
  });

  final MusicChannel channel;
  final MusicQueueItem item;
  final int index;
  final Widget? dragHandle;

  /// Puts the item before another one, or at the end (null).
  void _moveBefore(BuildContext context, WidgetRef ref, String? beforeItemId) =>
      moveQueueItem(context, ref, channel, item.id, beforeItemId);

  void _menu(BuildContext context, WidgetRef ref, Offset position) {
    final access = ref.read(musicAccessProvider(channel));
    final manage = access.canManageQueue;
    final queue =
        ref.read(musicSessionProvider(channel)).value?.queue ??
        const <MusicQueueItem>[];
    final at = queue.indexWhere((each) => each.id == item.id);
    void onMove(String? beforeItemId) =>
        _moveBefore(context, ref, beforeItemId);
    final uri = item.track.uri;
    showOcMenu(
      context: context,
      position: position,
      entries: [
        if (manage) ...[
          OcMenuItem(
            label: 'Play next',
            icon: OcIcons.verticalAlignTop,
            onSelected: at <= 0 ? null : () => onMove(queue.first.id),
          ),
          OcMenuItem(
            label: 'Move up',
            icon: OcIcons.arrowUpward,
            onSelected: at <= 0 ? null : () => onMove(queue[at - 1].id),
          ),
          OcMenuItem(
            label: 'Move down',
            icon: OcIcons.arrowDownward,
            onSelected: at < 0 || at == queue.length - 1
                ? null
                : () => onMove(at + 2 < queue.length ? queue[at + 2].id : null),
          ),
          const OcMenuDivider(),
        ],
        OcMenuItem(
          label: 'Remove',
          icon: OcIcons.delete,
          onSelected: access.canRemove(item.requesterId)
              ? () => musicAction(
                  context,
                  ref,
                  channel,
                  (repository) => repository.musicRemove(
                    channel.server,
                    channel.channel,
                    [item.id],
                  ),
                )
              : null,
        ),
        if (uri.isNotEmpty) ...[
          const OcMenuDivider(),
          OcMenuItem(
            label: 'Copy link',
            icon: OcIcons.link,
            onSelected: () {
              Clipboard.setData(ClipboardData(text: uri));
              showOcToast(context, 'Link copied');
            },
          ),
          OcMenuItem(
            label: 'Open link',
            icon: OcIcons.openInNew,
            onSelected: () => openMessageLink(context, ref, uri),
          ),
        ],
      ],
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final requester = ref.watch(
      serverProvider(
        channel.server,
      ).select((state) => state.data?.members[item.requesterId]?.displayName),
    );
    final track = item.track;
    final subtitle = [
      if (track.artist.isNotEmpty) track.artist,
      track.isStream ? 'LIVE' : formatTrackTime(track.duration),
    ].join(' · ');
    return RepaintBoundary(
      child: Builder(
        builder: (row) => Hoverable(
          onSecondaryTap: (position) => _menu(row, ref, position),
          semanticLabel:
              '${index + 1}. ${track.title}, $subtitle, added by '
              '${requester ?? 'someone'}',
          builder: (context, state) => AnimatedContainer(
            duration: OcMotion.of(context).hover,
            height: _rowHeight,
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
            decoration: BoxDecoration(
              color: state.active ? colors.hover : null,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Row(
              children: [
                ?dragHandle,
                MusicArtwork(
                  serverKey: channel.server,
                  artworkId: track.artworkId,
                  size: 40,
                ),
                const SizedBox(width: OcSpace.s10),
                Expanded(
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        track.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.body.copyWith(color: colors.text),
                      ),
                      Text(
                        subtitle,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.small.copyWith(color: colors.textMuted),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: OcSpace.s8),
                OcAvatar(
                  id: item.requesterId,
                  name: requester ?? '?',
                  size: 20,
                ),
                Builder(
                  builder: (button) => OcIconButton(
                    icon: OcIcons.moreHoriz,
                    tooltip: 'More',
                    size: OcIconButtonSize.compact,
                    onPressed: () => _menu(
                      button,
                      ref,
                      (button.findRenderObject()! as RenderBox).localToGlobal(
                        Offset.zero,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
