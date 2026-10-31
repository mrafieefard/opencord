import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/music/music_access.dart';
import 'package:opencord/features/music/music_artwork.dart';
import 'package:opencord/features/music/music_drop.dart';
import 'package:opencord/features/music/music_source_icon.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// A search in a channel's music panel: what was asked, from where, and
/// what came back; no results while not searching (§19.4).
@immutable
class MusicSearchState {
  const MusicSearchState({this.source = '', this.query = '', this.results});

  /// Empty for the server's default.
  final String source;
  final String query;

  /// Null while not searching; loading, the results, or why not.
  final AsyncValue<MusicSearchResults>? results;

  bool get searching => results != null;
}

class MusicSearchNotifier extends Notifier<MusicSearchState> {
  MusicSearchNotifier(this.channel);

  final MusicChannel channel;

  @override
  MusicSearchState build() => const MusicSearchState();

  void chooseSource(String source) =>
      state = MusicSearchState(source: source, query: state.query);

  Future<void> search(String query) async {
    final words = query.trim();
    if (words.isEmpty) return close();
    state = MusicSearchState(
      source: state.source,
      query: words,
      results: const AsyncLoading(),
    );
    final found = await AsyncValue.guard(
      () => ref
          .read(repositoryProvider)
          .musicSearch(
            channel.server,
            channel.channel,
            words,
            source: state.source,
          ),
    );
    // A newer search, or Back to queue, meanwhile.
    if (!ref.mounted || state.query != words || !state.searching) return;
    state = MusicSearchState(
      source: state.source,
      query: words,
      results: found,
    );
  }

  /// Searches again for what was asked: an uploaded song was removed.
  Future<void> refresh() async {
    if (state.searching) await search(state.query);
  }

  /// Back to the queue.
  void close() => state = MusicSearchState(source: state.source);
}

final musicSearchProvider = NotifierProvider.autoDispose
    .family<MusicSearchNotifier, MusicSearchState, MusicChannel>(
      MusicSearchNotifier.new,
    );

/// "Add a song": the field with the source selector on the left and debounced search.
class MusicSearchField extends ConsumerStatefulWidget {
  const MusicSearchField({
    super.key,
    required this.channel,
    this.autofocus = false,
  });

  final MusicChannel channel;
  final bool autofocus;

  @override
  ConsumerState<MusicSearchField> createState() => _MusicSearchFieldState();
}

class _MusicSearchFieldState extends ConsumerState<MusicSearchField> {
  final _controller = TextEditingController();
  Timer? _debounceTimer;

  @override
  void dispose() {
    _debounceTimer?.cancel();
    _controller.dispose();
    super.dispose();
  }

  void _onChanged(String query) {
    _debounceTimer?.cancel();
    final words = query.trim();
    if (words.isEmpty) {
      ref.read(musicSearchProvider(widget.channel).notifier).close();
      return;
    }
    _debounceTimer = Timer(const Duration(seconds: 1), () {
      if (!mounted) return;
      ref.read(musicSearchProvider(widget.channel).notifier).search(words);
    });
  }

  void _pickSource(BuildContext anchor, List<String> sources, String chosen) {
    final notifier = ref.read(musicSearchProvider(widget.channel).notifier);
    showOcMenu(
      context: anchor,
      position: globalRectOf(anchor).bottomLeft.translate(0, 4),
      entries: [
        for (final key in sources)
          OcMenuItem(
            label: musicSourceLabel(key),
            leading: MusicSourceIcon(
              key,
              size: 16,
              color: key == chosen ? context.oc.text : context.oc.textSecondary,
            ),
            checked: key == chosen,
            onSelected: () {
              notifier.chooseSource(key);
              final words = _controller.text.trim();
              if (words.isNotEmpty) {
                _debounceTimer?.cancel();
                notifier.search(words);
              }
            },
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final access = ref.watch(musicAccessProvider(widget.channel));
    final settings = ref
        .watch(musicSettingsProvider(widget.channel.server))
        .value;
    final sources = settings == null
        ? const <String>[]
        : searchableSources(settings);
    final source = ref.watch(
      musicSearchProvider(widget.channel).select((state) => state.source),
    );
    final chosen = source.isNotEmpty
        ? source
        : sources.contains(settings?.defaultSource)
        ? settings!.defaultSource
        : sources.firstOrNull ?? '';

    final selector = sources.isEmpty
        ? null
        : Builder(
            builder: (pill) => Hoverable(
              onTap: access.canAdd && sources.length > 1
                  ? () => _pickSource(pill, sources, chosen)
                  : null,
              semanticLabel: 'Search ${musicSourceLabel(chosen)}',
              focusRadius: BorderRadius.circular(OcRadius.searchPill),
              builder: (context, state) => AnimatedContainer(
                duration: OcMotion.of(context).hover,
                margin: const EdgeInsets.only(
                  left: OcSpace.s6,
                  right: OcSpace.s2,
                ),
                padding: const EdgeInsets.symmetric(
                  horizontal: OcSpace.s6,
                  vertical: OcSpace.s4,
                ),
                decoration: BoxDecoration(
                  color: state.active && sources.length > 1
                      ? colors.selected
                      : colors.hover,
                  borderRadius: BorderRadius.circular(OcRadius.searchPill),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    MusicSourceIcon(chosen, size: 18),
                    if (sources.length > 1) ...[
                      const SizedBox(width: OcSpace.s2),
                      Icon(
                        OcIcons.expandMore,
                        size: OcSize.iconInline,
                        color: colors.textMuted,
                      ),
                    ],
                  ],
                ),
              ),
            ),
          );

    final field = OcTextField(
      controller: _controller,
      // Without Lavalink, search finds uploaded songs only.
      hint: settings?.hasNodes ?? true ? 'Add a song' : 'Find an uploaded song',
      prefix: selector,
      autofocus: widget.autofocus && access.canAdd,
      enabled: access.canAdd,
      maxLength: 200,
      semanticLabel: 'Search for music',
      textInputAction: TextInputAction.search,
      onChanged: _onChanged,
      onSubmitted: (query) {
        _debounceTimer?.cancel();
        ref.read(musicSearchProvider(widget.channel).notifier).search(query);
      },
    );
    return access.canAdd
        ? field
        : Tooltip(
            message:
                access.why(Permissions.musicAdd) ?? 'You can’t add music here',
            child: field,
          );
  }
}

/// Search results, replacing Up next while searching: skeleton rows while
/// loading, a playlist as one card with Add all.
class MusicSearchResultsList extends ConsumerWidget {
  const MusicSearchResultsList({super.key, required this.channel});

  final MusicChannel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final results = ref.watch(
      musicSearchProvider(channel).select((state) => state.results),
    );
    switch (results) {
      case null:
        return const SliverToBoxAdapter(child: SizedBox.shrink());
      case AsyncError(:final error):
        return SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.all(OcSpace.s16),
            child: Text(
              error is RepoException
                  ? error.message
                  : 'The search didn’t work. Try again.',
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
          ),
        );
      case AsyncData(:final value) when value.results.isEmpty:
        return SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.all(OcSpace.s16),
            child: Text(
              'Nothing found',
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
          ),
        );
      case AsyncData(:final value):
        final playlist = value.playlist;
        return SliverList.builder(
          itemCount: value.results.length + (playlist == null ? 0 : 1),
          itemBuilder: (context, index) {
            if (playlist != null && index == 0) {
              return _PlaylistCard(channel: channel, playlist: playlist);
            }
            final result = value.results[index - (playlist == null ? 0 : 1)];
            return MusicResultRow(channel: channel, result: result);
          },
        );
      default:
        return SliverList.builder(
          itemCount: 6,
          itemBuilder: (context, index) => const _SkeletonRow(),
        );
    }
  }
}

/// Adds results, then goes back to the queue.
Future<void> addResults(
  BuildContext context,
  WidgetRef ref,
  MusicChannel channel,
  List<String> resultIds, {
  MusicPlacement placement = MusicPlacement.end,
}) async {
  await musicAction(
    context,
    ref,
    channel,
    (repository) => repository.musicEnqueue(
      channel.server,
      channel.channel,
      resultIds,
      placement: placement,
    ),
  );
}

/// A track found: artwork, title, "artist · duration", its source, Add.
class MusicResultRow extends ConsumerWidget {
  const MusicResultRow({
    super.key,
    required this.channel,
    required this.result,
  });

  final MusicChannel channel;
  final MusicSearchResult result;

  void _menu(BuildContext context, WidgetRef ref, Offset position) {
    final manage = ref.read(musicAccessProvider(channel)).canManageQueue;
    final track = result.track;
    // Design 2026-10-24: one's own uploaded songs, or anyone's for an
    // administrator.
    final removable =
        track.isLocal &&
        (track.removable ||
            (ref
                    .read(serverProvider(channel.server))
                    .data
                    ?.can(Permissions.administrator) ??
                false));
    showOcMenu(
      context: context,
      position: position,
      entries: [
        OcMenuItem(
          label: 'Add to queue',
          icon: OcIcons.add,
          onSelected: () => addResults(context, ref, channel, [result.id]),
        ),
        OcMenuItem(
          label: 'Play next',
          icon: OcIcons.verticalAlignTop,
          onSelected: manage
              ? () => addResults(context, ref, channel, [
                  result.id,
                ], placement: MusicPlacement.next)
              : null,
        ),
        OcMenuItem(
          label: 'Play now',
          icon: OcIcons.playArrow,
          onSelected: manage
              ? () => addResults(context, ref, channel, [
                  result.id,
                ], placement: MusicPlacement.now)
              : null,
        ),
        if (removable) ...[
          const OcMenuDivider(),
          OcMenuItem(
            label: 'Remove from server',
            icon: OcIcons.delete,
            onSelected: () async {
              if (await removeLocalSong(context, ref, channel.server, track)) {
                await ref.read(musicSearchProvider(channel).notifier).refresh();
              }
            },
          ),
        ],
      ],
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final track = result.track;
    final access = ref.watch(musicAccessProvider(channel));
    final subtitle = [
      if (track.artist.isNotEmpty) track.artist,
      track.isStream ? 'LIVE' : formatTrackTime(track.duration),
    ].join(' · ');
    final row = Builder(
      builder: (row) => Hoverable(
        onSecondaryTap: (position) => _menu(row, ref, position),
        semanticLabel: '${track.title}, $subtitle, ${track.sourceName}',
        builder: (context, state) => AnimatedContainer(
          duration: OcMotion.of(context).hover,
          height: 52,
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
          decoration: BoxDecoration(
            color: state.active ? colors.hover : null,
            borderRadius: BorderRadius.circular(8),
          ),
          child: Row(
            children: [
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
              if (track.sourceName.isNotEmpty) ...[
                const SizedBox(width: OcSpace.s6),
                SourcePill(track.sourceName),
              ],
              OcIconButton(
                icon: OcIcons.add,
                tooltip: 'Add to queue',
                size: OcIconButtonSize.compact,
                onPressed: access.canAdd
                    ? () => addResults(context, ref, channel, [result.id])
                    : null,
              ),
            ],
          ),
        ),
      ),
    );
    if (!access.canManageQueue) return row;
    // Dropped on the cover, it plays now (§19.5).
    return Draggable<MusicDrop>(
      data: MusicDrop(resultIds: [result.id]),
      feedback: _DragFeedback(title: track.title),
      child: row,
    );
  }
}

/// What a drag carries: search results, or recently played tracks.
@immutable
class MusicDrop {
  const MusicDrop({this.resultIds = const [], this.historyItemIds = const []});

  final List<String> resultIds;
  final List<String> historyItemIds;
}

class _DragFeedback extends StatelessWidget {
  const _DragFeedback({required this.title});

  final String title;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Material(
      color: colors.elevated,
      borderRadius: BorderRadius.circular(8),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s8,
        ),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 240),
          child: Text(
            title,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: OcText.body.copyWith(color: colors.text),
          ),
        ),
      ),
    );
  }
}

/// A source as text, never a logo (§15.3).
class SourcePill extends StatelessWidget {
  const SourcePill(this.name, {super.key});

  final String name;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: OcSpace.s6, vertical: 1),
      decoration: BoxDecoration(
        border: Border.all(color: colors.border),
        borderRadius: BorderRadius.circular(OcRadius.searchPill),
      ),
      child: Text(
        name,
        style: OcText.meta.copyWith(color: colors.textSecondary),
      ),
    );
  }
}

class _PlaylistCard extends ConsumerWidget {
  const _PlaylistCard({required this.channel, required this.playlist});

  final MusicChannel channel;
  final MusicPlaylist playlist;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final access = ref.watch(musicAccessProvider(channel));
    final count = playlist.resultIds.length;
    return Container(
      margin: const EdgeInsets.only(bottom: OcSpace.s8),
      padding: const EdgeInsets.all(OcSpace.s12),
      decoration: BoxDecoration(
        border: Border.all(color: colors.border),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        children: [
          Icon(OcIcons.queueMusic, size: 28, color: colors.text),
          const SizedBox(width: OcSpace.s12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  playlist.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: OcText.bodyStrong.copyWith(color: colors.text),
                ),
                Text(
                  count == 1
                      ? 'Playlist · 1 track'
                      : 'Playlist · $count tracks',
                  style: OcText.small.copyWith(color: colors.textMuted),
                ),
              ],
            ),
          ),
          OcButton(
            label: 'Add all',
            dense: true,
            onPressed: access.canAdd
                ? () => addResults(context, ref, channel, playlist.resultIds)
                : null,
          ),
        ],
      ),
    );
  }
}

/// A grey row while results load (§19.9).
class _SkeletonRow extends StatelessWidget {
  const _SkeletonRow();

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    Widget bar(double width) => Container(
      width: width,
      height: 10,
      decoration: BoxDecoration(
        color: colors.hover,
        borderRadius: BorderRadius.circular(5),
      ),
    );
    return SizedBox(
      height: 52,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
        child: Row(
          children: [
            Container(
              width: 40,
              height: 40,
              decoration: BoxDecoration(
                color: colors.hover,
                borderRadius: BorderRadius.circular(6),
              ),
            ),
            const SizedBox(width: OcSpace.s10),
            Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [bar(150), const SizedBox(height: 6), bar(90)],
            ),
          ],
        ),
      ),
    );
  }
}
