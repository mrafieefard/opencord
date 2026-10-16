import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/music/music_access.dart';
import 'package:opencord/features/music/music_artwork.dart';
import 'package:opencord/features/music/music_controls.dart';
import 'package:opencord/features/music/music_queue.dart';
import 'package:opencord/features/music/music_search.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// People · Music, under the right-side panel's header when a voice
/// channel is involved (§19.2).
class RightPanelModeBar extends ConsumerWidget {
  const RightPanelModeBar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) => Padding(
    padding: const EdgeInsets.fromLTRB(OcSpace.s12, OcSpace.s8, OcSpace.s12, 0),
    child: ChoiceChips<RightPanelMode>(
      options: const [
        (RightPanelMode.people, 'People'),
        (RightPanelMode.music, 'Music'),
      ],
      value: ref.watch(rightPanelModeProvider),
      onChanged: (mode) => ref.read(rightPanelModeProvider.notifier).set(mode),
    ),
  );
}

/// Whether the right-side panel offers Music for [channel]: music is set
/// up there, or this member can set it up.
final musicOfferedProvider = Provider.autoDispose.family<bool, MusicChannel>((
  ref,
  channel,
) {
  final configured = ref.watch(musicConfiguredProvider(channel.server));
  return configured ||
      ref.watch(
        serverProvider(
          channel.server,
        ).select((state) => state.data?.can(Permissions.manageServer) ?? false),
      );
});

/// The music panel (§19.4): the track in the middle, what comes next
/// underneath. It replaces the users list, 340 px wide.
class MusicPanel extends ConsumerWidget {
  const MusicPanel({
    super.key,
    required this.channel,
    required this.onClose,
    this.trailingControls = false,
  });

  final MusicChannel channel;
  final VoidCallback onClose;
  final bool trailingControls;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(
      serverProvider(
        channel.server,
      ).select((state) => state.data?.channels[channel.channel]?.name),
    );
    final configured = ref.watch(musicConfiguredProvider(channel.server));
    return ColoredBox(
      color: colors.sidebar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          HeaderBar(
            trailingControls: trailingControls,
            padding: const EdgeInsets.only(
              left: OcSpace.s16,
              right: OcSpace.s8,
            ),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    'Music · ${name ?? ''}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.header.copyWith(color: colors.text),
                  ),
                ),
                if (configured) _PanelMenu(channel: channel),
                if (!trailingControls ||
                    !showsTrailingWindowControls(context, ref))
                  OcIconButton(
                    icon: OcIcons.close,
                    tooltip: 'Close',
                    size: OcIconButtonSize.compact,
                    onPressed: onClose,
                  ),
              ],
            ),
          ),
          const RightPanelModeBar(),
          Expanded(
            child: configured
                ? _MusicBody(channel: channel)
                : _NotSetUp(serverKey: channel.server),
          ),
        ],
      ),
    );
  }
}

/// "⋯": Stop player, Music settings, Expand.
class _PanelMenu extends ConsumerWidget {
  const _PanelMenu({required this.channel});

  final MusicChannel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final access = ref.watch(musicAccessProvider(channel));
    final expanded = ref.watch(musicExpandedProvider);
    final settings =
        access.permissions.has(Permissions.musicManage) || access.manageServer;
    return Builder(
      builder: (button) => OcIconButton(
        icon: OcIcons.moreHoriz,
        tooltip: 'More',
        size: OcIconButtonSize.compact,
        onPressed: () => showOcMenu(
          context: button,
          position: globalRectOf(button).bottomLeft.translate(0, 4),
          entries: [
            OcMenuItem(
              label: 'Stop player',
              icon: OcIcons.stop,
              onSelected: access.manage
                  ? () => musicAction(
                      context,
                      ref,
                      channel,
                      (repository) =>
                          repository.musicStop(channel.server, channel.channel),
                    )
                  : null,
            ),
            if (settings)
              OcMenuItem(
                label: 'Music settings',
                icon: OcIcons.settings,
                onSelected: () => showServerSettings(
                  context,
                  ref,
                  serverKey: channel.server,
                  page: 'music',
                ),
              ),
            OcMenuItem(
              label: expanded ? 'Collapse' : 'Expand',
              icon: expanded ? OcIcons.closeFullscreen : OcIcons.openInFull,
              onSelected: () =>
                  ref.read(musicExpandedProvider.notifier).set(!expanded),
            ),
          ],
        ),
      ),
    );
  }
}

/// Music isn't set up: who manages the server can do it (§19.9).
class _NotSetUp extends ConsumerWidget {
  const _NotSetUp({required this.serverKey});

  final String serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.all(OcSpace.s24),
      child: Column(
        children: [
          Icon(OcIcons.libraryMusic, size: 40, color: colors.textMuted),
          const SizedBox(height: OcSpace.s12),
          Text(
            'Music isn’t set up on this server yet',
            textAlign: TextAlign.center,
            style: OcText.body.copyWith(color: colors.textSecondary),
          ),
          const SizedBox(height: OcSpace.s16),
          OcButton.primary(
            label: 'Set up music',
            onPressed: () => showServerSettings(
              context,
              ref,
              serverKey: serverKey,
              page: 'music',
            ),
          ),
        ],
      ),
    );
  }
}

class _MusicBody extends ConsumerStatefulWidget {
  const _MusicBody({required this.channel});

  final MusicChannel channel;

  @override
  ConsumerState<_MusicBody> createState() => _MusicBodyState();
}

class _MusicBodyState extends ConsumerState<_MusicBody> {
  var _history = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final channel = widget.channel;
    final loaded = ref.watch(
      musicSessionProvider(channel).select(
        (value) => switch (value) {
          AsyncData() => null,
          AsyncError(:final error) => error,
          _ => false,
        },
      ),
    );
    if (loaded == false) {
      return const Center(child: OcSpinner(size: 20));
    }
    if (loaded != null) {
      return Padding(
        padding: const EdgeInsets.all(OcSpace.s16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            InlineError(
              loaded is RepoException
                  ? loaded.message
                  : 'The music could not be loaded.',
            ),
            const SizedBox(height: OcSpace.s12),
            OcButton(
              label: 'Try again',
              onPressed: () =>
                  ref.read(musicSessionProvider(channel).notifier).retry(),
            ),
          ],
        ),
      );
    }
    final (playing, waiting, queued, played) = ref.watch(
      musicSessionProvider(channel).select((value) {
        final session = value.value;
        return (
          session?.nowPlaying != null,
          session?.waitingForNode ?? false,
          session?.queue.isNotEmpty ?? false,
          session?.history.length ?? 0,
        );
      }),
    );
    final access = ref.watch(musicAccessProvider(channel));
    final searching = ref.watch(
      musicSearchProvider(channel).select((state) => state.searching),
    );
    final pad = const EdgeInsets.symmetric(horizontal: OcSpace.s12);
    return CustomScrollView(
      slivers: [
        if (waiting)
          const SliverToBoxAdapter(
            child: Padding(
              padding: EdgeInsets.fromLTRB(
                OcSpace.s12,
                OcSpace.s12,
                OcSpace.s12,
                0,
              ),
              child: InlineError('Music server unavailable, retrying…'),
            ),
          ),
        if (!access.present)
          SliverToBoxAdapter(child: _JoinToControl(channel: channel)),
        SliverToBoxAdapter(child: NowPlayingView(channel: channel)),
        SliverPadding(
          padding: pad.copyWith(top: OcSpace.s16, bottom: OcSpace.s8),
          sliver: SliverToBoxAdapter(
            child: MusicSearchField(channel: channel, autofocus: !playing),
          ),
        ),
        if (searching) ...[
          SliverPadding(
            padding: pad,
            sliver: SliverToBoxAdapter(
              child: _SectionTitle(
                'Results',
                action: (
                  'Back to queue',
                  () => ref.read(musicSearchProvider(channel).notifier).close(),
                ),
              ),
            ),
          ),
          SliverPadding(
            padding: pad,
            sliver: MusicSearchResultsList(channel: channel),
          ),
        ] else ...[
          if (queued)
            SliverPadding(
              padding: pad,
              sliver: SliverToBoxAdapter(child: _UpNextTitle(channel: channel)),
            ),
          SliverPadding(
            padding: pad,
            sliver: MusicQueueList(channel: channel),
          ),
          if (played > 0)
            SliverPadding(
              padding: pad.copyWith(top: OcSpace.s8),
              sliver: SliverToBoxAdapter(
                child: Hoverable(
                  onTap: () => setState(() => _history = !_history),
                  semanticLabel: _history
                      ? 'Hide recently played'
                      : 'Show recently played',
                  builder: (context, state) => Padding(
                    padding: const EdgeInsets.symmetric(vertical: OcSpace.s8),
                    child: Row(
                      children: [
                        Icon(
                          OcIcons.history,
                          size: OcSize.iconInline,
                          color: colors.textSecondary,
                        ),
                        const SizedBox(width: OcSpace.s6),
                        Text(
                          'Recently played',
                          style: OcText.small.copyWith(
                            color: state.active
                                ? colors.text
                                : colors.textSecondary,
                          ),
                        ),
                        Icon(
                          _history ? OcIcons.expandLess : OcIcons.expandMore,
                          size: OcSize.iconInline,
                          color: colors.textMuted,
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          if (_history && played > 0)
            SliverPadding(
              padding: pad,
              sliver: _HistoryList(channel: channel),
            ),
        ],
        const SliverToBoxAdapter(child: SizedBox(height: OcSpace.s16)),
      ],
    );
  }
}

/// Read-only (§19.4): not in the channel.
class _JoinToControl extends ConsumerWidget {
  const _JoinToControl({required this.channel});

  final MusicChannel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return Container(
      margin: const EdgeInsets.fromLTRB(
        OcSpace.s12,
        OcSpace.s12,
        OcSpace.s12,
        0,
      ),
      padding: const EdgeInsets.all(OcSpace.s12),
      decoration: BoxDecoration(
        border: Border.all(color: colors.border),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              'Join the voice channel to control music',
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
          ),
          const SizedBox(width: OcSpace.s8),
          OcButton(
            label: 'Join',
            dense: true,
            onPressed: () =>
                joinVoice(context, ref, channel.server, channel.channel),
          ),
        ],
      ),
    );
  }
}

/// The cover, the track and its controls; "Nothing playing" without one.
/// Dropping a track on the cover plays it now (§19.5).
class NowPlayingView extends ConsumerWidget {
  const NowPlayingView({
    super.key,
    required this.channel,
    this.coverSize = 240,
  });

  final MusicChannel channel;
  final double coverSize;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final item = ref.watch(
      musicSessionProvider(channel).select((value) => value.value?.nowPlaying),
    );
    final notice = ref.watch(
      musicNoticesProvider(channel).select((notices) => notices.firstOrNull),
    );
    final requester = item == null
        ? null
        : ref.watch(
            serverProvider(channel.server).select(
              (state) => state.data?.members[item.requesterId]?.displayName,
            ),
          );
    final access = ref.watch(musicAccessProvider(channel));
    final track = item?.track;
    final cover = LayoutBuilder(
      builder: (context, constraints) => MusicArtwork(
        serverKey: channel.server,
        artworkId: track?.artworkId ?? '',
        size: coverSize.clamp(96, constraints.maxWidth),
        radius: 12,
      ),
    );
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        OcSpace.s16,
        OcSpace.s16,
        OcSpace.s16,
        0,
      ),
      child: Column(
        children: [
          DragTarget<MusicDrop>(
            onWillAcceptWithDetails: (_) => access.canManageQueue,
            onAcceptWithDetails: (details) => musicAction(
              context,
              ref,
              channel,
              (repository) => repository.musicEnqueue(
                channel.server,
                channel.channel,
                details.data.resultIds,
                historyItemIds: details.data.historyItemIds,
                placement: MusicPlacement.now,
              ),
            ),
            builder: (context, candidates, _) => DecoratedBox(
              position: DecorationPosition.foreground,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(12),
                border: candidates.isEmpty
                    ? null
                    : Border.all(color: colors.text, width: 2),
              ),
              child: cover,
            ),
          ),
          const SizedBox(height: OcSpace.s12),
          Text(
            track?.title ?? 'Nothing playing',
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.center,
            style: OcText.title.copyWith(color: colors.text),
          ),
          if (track != null && track.artist.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: OcSpace.s2),
              child: Text(
                track.artist,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ),
          if (track != null) ...[
            const SizedBox(height: OcSpace.s8),
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                if (track.sourceName.isNotEmpty) ...[
                  SourcePill(track.sourceName),
                  const SizedBox(width: OcSpace.s8),
                ],
                Text(
                  'Added by ',
                  style: OcText.meta.copyWith(color: colors.textMuted),
                ),
                OcAvatar(
                  id: item!.requesterId,
                  name: requester ?? '?',
                  size: 16,
                ),
                const SizedBox(width: OcSpace.s4),
                Flexible(
                  child: Text(
                    requester ?? 'someone',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.meta.copyWith(color: colors.textSecondary),
                  ),
                ),
              ],
            ),
          ],
          const SizedBox(height: OcSpace.s8),
          MusicProgress(channel: channel),
          const SizedBox(height: OcSpace.s4),
          Stack(
            alignment: Alignment.center,
            children: [
              MusicTransport(channel: channel),
              const Align(
                alignment: Alignment.centerRight,
                child: MusicVolumeButton(),
              ),
            ],
          ),
          if (notice != null)
            Padding(
              padding: const EdgeInsets.only(top: OcSpace.s8),
              child: Text(
                notice.text,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: OcText.meta.copyWith(color: colors.textMuted),
              ),
            ),
        ],
      ),
    );
  }
}

class _SectionTitle extends StatelessWidget {
  const _SectionTitle(this.title, {this.detail, this.action});

  final String title;
  final String? detail;
  final (String, VoidCallback?)? action;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: OcSpace.s6),
      child: Row(
        children: [
          Text(
            title,
            style: OcText.label.copyWith(color: colors.textSecondary),
          ),
          if (detail != null) ...[
            const SizedBox(width: OcSpace.s8),
            Expanded(
              child: Text(
                detail!,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.meta.copyWith(color: colors.textMuted),
              ),
            ),
          ] else
            const Spacer(),
          if (action case (final label, final onTap))
            OcButton.ghost(label: label, dense: true, onPressed: onTap),
        ],
      ),
    );
  }
}

/// "Up next", "12 tracks · 47 min" and Clear.
class _UpNextTitle extends ConsumerWidget {
  const _UpNextTitle({required this.channel});

  final MusicChannel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final summary = ref.watch(
      musicSessionProvider(channel).select(
        (value) => value.value == null ? '' : queueSummary(value.value!),
      ),
    );
    final access = ref.watch(musicAccessProvider(channel));
    return _SectionTitle(
      'Up next',
      detail: summary,
      action: access.canManageQueue
          ? (
              'Clear',
              () => musicAction(
                context,
                ref,
                channel,
                (repository) =>
                    repository.musicClear(channel.server, channel.channel),
              ),
            )
          : null,
    );
  }
}

/// The last 50 played, each with Add again.
class _HistoryList extends ConsumerWidget {
  const _HistoryList({required this.channel});

  final MusicChannel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final history = ref.watch(
      musicSessionProvider(
        channel,
      ).select((value) => value.value?.history ?? const <MusicQueueItem>[]),
    );
    final access = ref.watch(musicAccessProvider(channel));
    return SliverList.builder(
      itemCount: history.length,
      itemBuilder: (context, index) {
        final item = history[index];
        final row = SizedBox(
          height: 52,
          child: Row(
            children: [
              MusicArtwork(
                serverKey: channel.server,
                artworkId: item.track.artworkId,
                size: 40,
              ),
              const SizedBox(width: OcSpace.s10),
              Expanded(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      item.track.title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.body.copyWith(color: colors.text),
                    ),
                    Text(
                      item.track.artist,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                  ],
                ),
              ),
              OcButton.ghost(
                label: 'Add again',
                dense: true,
                onPressed: access.canAdd
                    ? () => musicAction(
                        context,
                        ref,
                        channel,
                        (repository) => repository.musicEnqueue(
                          channel.server,
                          channel.channel,
                          const [],
                          historyItemIds: [item.id],
                        ),
                      )
                    : null,
              ),
            ],
          ),
        );
        if (!access.canManageQueue) return row;
        return Draggable<MusicDrop>(
          data: MusicDrop(historyItemIds: [item.id]),
          feedback: Material(
            color: colors.elevated,
            borderRadius: BorderRadius.circular(8),
            child: Padding(
              padding: const EdgeInsets.all(OcSpace.s8),
              child: Text(
                item.track.title,
                style: OcText.body.copyWith(color: colors.text),
              ),
            ),
          ),
          child: row,
        );
      },
    );
  }
}
