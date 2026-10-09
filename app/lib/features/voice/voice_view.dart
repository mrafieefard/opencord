import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/voice/voice_focus.dart';
import 'package:opencord/features/voice/voice_layout.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

/// The main area for a voice channel (§4.10): its tiles and controls once
/// connected, a prompt to join before. Runs on mock data in Phase 1.
class VoiceView extends ConsumerWidget {
  const VoiceView({super.key, required this.channel});

  final ChannelRef channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!ref.read(repositoryProvider).capabilities.voice) {
      return _ComingSoon(channel: channel);
    }
    final connected = ref.watch(
      voiceSessionProvider.select(
        (voice) =>
            voice.serverKey == channel.server &&
            voice.channelId == channel.channel,
      ),
    );
    final participants = ref.watch(
      voiceProvider(
        channel.server,
      ).select((voice) => voice[channel.channel] ?? const <VoiceParticipant>[]),
    );
    if (!connected) {
      return _JoinPrompt(channel: channel, participants: participants);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          child: _Stage(channel: channel, participants: participants),
        ),
        const Padding(
          padding: EdgeInsets.fromLTRB(
            OcSpace.s16,
            OcSpace.s4,
            OcSpace.s16,
            OcSpace.s20,
          ),
          child: Center(child: VoiceControlBar()),
        ),
      ],
    );
  }
}

/// The tiles: a grid, or one large with the rest in a strip (focus mode).
/// Tiles showing video ask for it at their size; nothing is asked for once
/// the stage is gone.
class _Stage extends ConsumerStatefulWidget {
  const _Stage({required this.channel, required this.participants});

  static const _gap = OcSpace.s8;
  static const _strip = Size(160, 90);

  final ChannelRef channel;
  final List<VoiceParticipant> participants;

  @override
  ConsumerState<_Stage> createState() => _StageState();
}

class _StageState extends ConsumerState<_Stage> {
  late final VideoWantsNotifier _wants;

  @override
  void initState() {
    super.initState();
    _wants = ref.read(videoWantsProvider.notifier);
  }

  @override
  void dispose() {
    final wants = _wants;
    WidgetsBinding.instance.addPostFrameCallback((_) => wants.set(const []));
    super.dispose();
  }

  /// After layout, when every tile's size is known.
  void _report(List<VideoWant> wants) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _wants.set(wants);
    });
  }

  @override
  Widget build(BuildContext context) {
    final channel = widget.channel;
    final participants = widget.participants;
    final feeds = ref.watch(videoFeedsProvider);
    final self = ref.watch(
      serverProvider(channel.server).select((state) => state.data?.self.id),
    );
    final pixels = MediaQuery.devicePixelRatioOf(context);
    final members =
        ref.watch(
          serverProvider(channel.server).select((state) => state.data?.members),
        ) ??
        const <int, Member>{};
    final speaking = ref.watch(speakingProvider(channel.server));
    final focus = voiceFocusProvider(channel);
    final tiles = voiceTiles(participants);
    final focused = switch (ref.watch(focus)) {
      final tile? when tiles.contains(tile) => tile,
      _ => null,
    };
    final byUser = {
      for (final participant in participants) participant.userId: participant,
    };
    final streams = ref.watch(streamsProvider(channel.server));
    final watching = ref.watch(watchingProvider);
    final ownShare = ref.watch(ownScreenShareProvider);
    final viewers = ref.watch(ownStreamViewersProvider);
    final wants = <VideoWant>[];
    Widget tile(VoiceTileId id, Size size, {bool small = false}) {
      final participant = byUser[id.userId];
      final own = id.userId == self;
      final stream = id.screen && !own
          ? streams.values
                .where(
                  (stream) =>
                      stream.userId == id.userId &&
                      stream.channelId == channel.channel,
                )
                .firstOrNull
          : null;
      final watched = stream != null && watching.contains(stream.key);
      final video = switch (id) {
        (screen: false, :final userId) => feeds.of(userId, self: self),
        _ when own => ownShare?.preview,
        (screen: true, :final userId) when watched => feeds.screens[userId],
        _ => null,
      };
      if (video != null) {
        wants.add(
          VideoWant(
            trackId: video.trackId,
            width: (size.width * pixels).round(),
            height: (size.height * pixels).round(),
          ),
        );
      }
      return SizedBox.fromSize(
        size: size,
        child: VoiceTileView(
          key: ValueKey(id),
          userId: id.userId,
          name: members[id.userId]?.displayName ?? 'Someone',
          screen: id.screen,
          muted: participant?.muted ?? false,
          deafened: participant?.deafened ?? false,
          camera: participant?.camera ?? false,
          video: video,
          speaking: speaking.contains(id.userId),
          focused: id == focused,
          small: small,
          onTap: () => ref.read(focus.notifier).toggle(id),
          onWatch: stream != null && !watched
              ? () => watchStream(
                  context,
                  ref,
                  stream.key,
                  // Watching opens the stream large (§9.5).
                  opened: () {
                    if (ref.read(focus) != id) {
                      ref.read(focus.notifier).toggle(id);
                    }
                  },
                )
              : null,
          onStopWatching: watched
              ? () {
                  ref.read(focus.notifier).clear();
                  ref.read(watchingProvider.notifier).unwatch(stream.key);
                }
              : null,
          viewers: id.screen && own && ownShare != null ? viewers.length : null,
          onViewers: () => _showViewers(context, viewers, members),
        ),
      );
    }

    return Padding(
      padding: const EdgeInsets.all(OcSpace.s16),
      child: LayoutBuilder(
        builder: (context, constraints) {
          wants.clear();
          final layout = _layout(constraints.biggest, tiles, focused, tile);
          _report(List.of(wants));
          return layout;
        },
      ),
    );
  }

  /// Who watches this device's stream (§9.1), by name.
  void _showViewers(
    BuildContext context,
    List<int> viewers,
    Map<int, Member> members,
  ) {
    final colors = context.oc;
    showPopover<void>(
      context: context,
      anchor: globalRectOf(context),
      builder: (context) => ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 240, maxHeight: 320),
        child: viewers.isEmpty
            ? Text(
                'No one is watching yet.',
                style: OcText.small.copyWith(color: colors.textSecondary),
              )
            : ListView(
                shrinkWrap: true,
                children: [
                  for (final id in viewers)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: OcSpace.s4),
                      child: Row(
                        children: [
                          OcAvatar(
                            id: id,
                            name: members[id]?.displayName ?? 'Someone',
                            size: 24,
                          ),
                          const SizedBox(width: OcSpace.s8),
                          Expanded(
                            child: Text(
                              members[id]?.displayName ?? 'Someone',
                              overflow: TextOverflow.ellipsis,
                              style: OcText.body.copyWith(color: colors.text),
                            ),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
      ),
    );
  }

  Widget _layout(
    Size area,
    List<VoiceTileId> tiles,
    VoiceTileId? focused,
    Widget Function(VoiceTileId id, Size size, {bool small}) tile,
  ) {
    const gap = _Stage._gap;
    const strip = _Stage._strip;
    if (focused != null) {
      final others = [
        for (final id in tiles)
          if (id != focused) id,
      ];
      final stripHeight = others.isEmpty ? 0.0 : strip.height + gap;
      final large = fitTiles(
        Size(area.width, math.max(0, area.height - stripHeight)),
        1,
      );
      return Column(
        children: [
          Expanded(child: Center(child: tile(focused, large))),
          if (others.isNotEmpty) ...[
            const SizedBox(height: gap),
            SizedBox(
              height: strip.height,
              child: Center(
                child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      for (final (index, id) in others.indexed) ...[
                        if (index > 0) const SizedBox(width: gap),
                        tile(id, strip, small: true),
                      ],
                    ],
                  ),
                ),
              ),
            ),
          ],
        ],
      );
    }
    final size = fitTiles(area, tiles.length, gap: gap);
    final columns = gridColumns(tiles.length);
    final rows = [
      for (var start = 0; start < tiles.length; start += columns)
        tiles.sublist(start, math.min(start + columns, tiles.length)),
    ];
    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        for (final (index, row) in rows.indexed) ...[
          if (index > 0) const SizedBox(height: gap),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              for (final (column, id) in row.indexed) ...[
                if (column > 0) const SizedBox(width: gap),
                tile(id, size),
              ],
            ],
          ),
        ],
      ],
    );
  }
}

/// Before joining (§4.10): the channel, who is in it, and Join voice.
class _JoinPrompt extends ConsumerWidget {
  const _JoinPrompt({required this.channel, required this.participants});

  static const _shownAvatars = 8;

  final ChannelRef channel;
  final List<VoiceParticipant> participants;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final data = ref.watch(
      serverProvider(channel.server).select((state) => state.data),
    );
    final name = data?.channels[channel.channel]?.name ?? '';
    final canJoin =
        data?.permissionsIn(channel.channel).has(Permissions.connect) ?? false;
    final extra = participants.length - _shownAvatars;
    return Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(OcSpace.s24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              name,
              textAlign: TextAlign.center,
              style: OcText.title.copyWith(color: colors.text),
            ),
            const SizedBox(height: OcSpace.s6),
            Text(
              participants.isEmpty
                  ? 'No one is here yet.'
                  : '${participants.length} connected',
              style: OcText.body.copyWith(color: colors.textSecondary),
            ),
            if (participants.isNotEmpty) ...[
              const SizedBox(height: OcSpace.s16),
              // Overlapping, like a stack of faces.
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  for (final participant in participants.take(_shownAvatars))
                    Align(
                      widthFactor: 0.8,
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          border: Border.all(color: colors.chat, width: 3),
                        ),
                        child: OcAvatar(
                          id: participant.userId,
                          name:
                              data?.members[participant.userId]?.displayName ??
                              'Someone',
                          size: 40,
                        ),
                      ),
                    ),
                  if (extra > 0)
                    Container(
                      width: 46,
                      height: 46,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: colors.hover,
                        border: Border.all(color: colors.chat, width: 3),
                      ),
                      child: Text(
                        '+$extra',
                        style: OcText.small.copyWith(
                          fontWeight: FontWeight.w600,
                          color: colors.text,
                        ),
                      ),
                    ),
                ],
              ),
            ],
            const SizedBox(height: OcSpace.s24),
            OcButton.primary(
              label: 'Join voice',
              onPressed: canJoin
                  ? () =>
                        joinVoice(context, ref, channel.server, channel.channel)
                  : null,
            ),
            if (!canJoin) ...[
              const SizedBox(height: OcSpace.s8),
              Text(
                "You don't have permission to join this channel.",
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// Voice on a server that does not have it yet (Phase 1 §9.3).
class _ComingSoon extends ConsumerWidget {
  const _ComingSoon({required this.channel});

  final ChannelRef channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(
      serverProvider(
        channel.server,
      ).select((state) => state.data?.channels[channel.channel]?.name),
    );
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(OcSpace.s24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(OcIcons.volumeUp, size: 40, color: colors.textMuted),
            const SizedBox(height: OcSpace.s12),
            Text(
              name ?? '',
              textAlign: TextAlign.center,
              style: OcText.title.copyWith(color: colors.text),
            ),
            const SizedBox(height: OcSpace.s6),
            Text(
              'Voice chat comes in a later version of Opencord.',
              textAlign: TextAlign.center,
              style: OcText.body.copyWith(color: colors.textSecondary),
            ),
          ],
        ),
      ),
    );
  }
}
