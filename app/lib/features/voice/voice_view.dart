import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/voice_view_prefs.dart';
import 'package:opencord/features/members/member_actions.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/music/music_stage.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/side_slot.dart';
import 'package:opencord/features/voice/voice_focus.dart';
import 'package:opencord/features/voice/voice_layout.dart';
import 'package:opencord/features/voice/user_audio_controls.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/ui/motion/flip_box.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
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
    final chat = ref.watch(
      serverProvider(channel.server).select(
        (state) => state.data?.channels[channel.channel]?.textInVoice ?? false,
      ),
    );
    final stage = _Stage(channel: channel, participants: participants);
    Widget content = Stack(
      children: [
        // The stage has the whole page, under what floats on it (§5.14). Laid
        // out at the width main ends at, it ignores a pane on its way (§4.4).
        Positioned.fill(
          child: KeyedSubtree(
            key: ShellKeys.stage,
            child: MainInnerWidth(child: stage),
          ),
        ),
        Positioned(
          left: 0,
          right: 0,
          bottom: 14,
          child: Align(
            alignment: Alignment.bottomCenter,
            child: VoiceControlBar(channel: channel, chat: chat),
          ),
        ),
      ],
    );
    if (VoiceDockActivity.maybeOf(context) == null) {
      content = VoiceDockActivityScope(child: content);
    }
    return content;
  }
}

/// The tiles: a grid, or one large with the rest in a strip (focus mode),
/// moving between the two on [FlipBox]es. Tiles showing video ask for it at
/// their size, which layout knows at once; nothing is asked for once the stage
/// is gone.
class _Stage extends ConsumerStatefulWidget {
  const _Stage({required this.channel, required this.participants});

  final ChannelRef channel;
  final List<VoiceParticipant> participants;

  @override
  ConsumerState<_Stage> createState() => _StageState();
}

class _StageState extends ConsumerState<_Stage> {
  late final VideoWantsNotifier _wants;

  /// Each tile on the stage, so it keeps its state and its video from the grid
  /// to the strip. A tile that left has none: it arrives again if it's back.
  final _tileKeys = <VoiceTileId, GlobalKey>{};

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
    final serverData = ref.watch(
      serverProvider(channel.server).select((state) => state.data),
    );
    final members = serverData?.members ?? const <int, Member>{};
    final speaking = ref.watch(speakingProvider(channel.server));
    final sounds = ref.watch(soundPlaysProvider(channel.server));
    final call = ref.watch(callUserAudioProvider);
    final mutedHere = {
      if (call.serverKey == channel.server)
        for (final MapEntry(key: userId, value: audio) in call.users.entries)
          if (audio.muted) userId,
    };
    final streamMutedHere = {
      if (call.serverKey == channel.server)
        for (final MapEntry(key: userId, value: audio) in call.users.entries)
          if (audio.streamMuted) userId,
    };
    final focus = voiceFocusProvider(channel);
    final videoOnly = ref.watch(
      voiceViewPrefsProvider.select((prefs) => prefs.hideNonVideo),
    );
    final tiles = voiceTiles(participants, videoOnly: videoOnly);
    final focused = switch (ref.watch(focus)) {
      final tile? when tiles.contains(tile) => tile,
      _ => null,
    };
    // Expand (Phase 3 §19.7): the music in the middle, the tiles below.
    final music =
        ref.watch(musicExpandedProvider) &&
            ref.watch(musicConfiguredProvider(channel.server))
        ? MusicStage(channel: channel)
        : null;
    final byUser = {
      for (final participant in participants) participant.userId: participant,
    };
    final streams = ref.watch(streamsProvider(channel.server));
    final watching = ref.watch(watchingProvider);
    final ownShare = ref.watch(ownScreenShareProvider);
    final viewers = ref.watch(ownStreamViewersProvider);
    final wants = <VideoWant>[];
    _tileKeys.removeWhere((id, _) => !tiles.contains(id));
    // What moves the tiles, not a resize or a pane on its way.
    final generation = '${tiles.join(',')}|$focused|${music != null}';
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
      final author = serverData?.authorOf(id.userId);
      return FlipBox(
        key: _tileKeys.putIfAbsent(id, GlobalKey.new),
        generation: generation,
        arriveDelay: FlipBox.stagger(tiles.indexOf(id)),
        child: VoiceTileView(
          key: ValueKey(id),
          userId: id.userId,
          name: author?.name ?? 'Someone',
          picture: avatarImage(
            ref,
            widget.channel.server,
            author?.avatarHash,
          ),
          screen: id.screen,
          muted:
              participant != null &&
              (participant.muted || participant.serverMuted),
          deafened:
              participant != null &&
              (participant.deafened || participant.serverDeafened),
          mutedHere: id.screen
              ? watched && streamMutedHere.contains(id.userId)
              : mutedHere.contains(id.userId),
          camera: participant?.camera ?? false,
          video: video,
          speaking: speaking.contains(id.userId),
          sound: id.screen ? null : sounds[id.userId],
          focused: id == focused,
          small: small,
          onTap: () => ref.read(focus.notifier).toggle(id),
          onSecondaryTap: switch (members[id.userId]) {
            // A watched stream: its sound, and stopping (§10.5).
            final _? when id.screen && watched => (position) => showOcMenu(
              context: context,
              position: position,
              entries: [
                if (stream.hasAudio)
                  OcMenuCustom(
                    (_) => StreamAudioControls(
                      serverKey: channel.server,
                      userId: id.userId,
                    ),
                  ),
                if (stream.hasAudio) const OcMenuDivider(),
                OcMenuItem(
                  label: 'Stop watching',
                  icon: OcIcons.visibilityOff,
                  onSelected: () {
                    ref.read(focus.notifier).clear();
                    ref.read(watchingProvider.notifier).unwatch(stream.key);
                  },
                ),
              ],
            ),
            final member? => (position) => MemberActions(
              context: context,
              ref: ref,
              serverKey: channel.server,
            ).showMenu(member, position),
            null => null,
          },
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

    if (tiles.isEmpty && videoOnly && music == null) {
      _report(const []);
      return const _NoVideo();
    }
    return LayoutBuilder(
      builder: (context, constraints) {
        wants.clear();
        final layout = _layout(
          constraints.biggest,
          tiles,
          focused,
          tile,
          music: music,
        );
        _report(List.of(wants));
        return layout;
      },
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
                          MemberAvatar(
                            serverKey: widget.channel.server,
                            userId: id,
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
    Size stage,
    List<VoiceTileId> tiles,
    VoiceTileId? focused,
    Widget Function(VoiceTileId id, Size size, {bool small}) tile, {
    Widget? music,
  }) {
    if (focused != null || music != null) {
      final others = [
        for (final id in tiles)
          if (id != focused || music != null) id,
      ];
      final area = const EdgeInsets.all(focusPadding).deflateSize(stage);
      final layout = focusLayout(area, others.length);
      return _group(const Offset(focusPadding, focusPadding) & area, [
        Positioned.fromRect(
          rect: layout.large,
          child: music ?? tile(focused!, layout.large.size),
        ),
        if (others.isNotEmpty)
          Positioned.fromRect(
            rect: layout.strip,
            child: _Strip(
              children: [
                for (final id in others)
                  SizedBox.fromSize(
                    key: ValueKey(id),
                    size: layout.stripTile,
                    child: tile(id, layout.stripTile, small: true),
                  ),
              ],
            ),
          ),
      ]);
    }
    final area = const EdgeInsets.all(gridPadding).deflateSize(stage);
    final layout = gridLayout(area, tiles.length);
    return _group(layout.group.shift(const Offset(gridPadding, gridPadding)), [
      for (final (index, id) in tiles.indexed)
        Positioned.fromRect(
          key: ValueKey(id),
          rect: layout.tiles[index],
          child: tile(id, layout.tiles[index].size),
        ),
    ]);
  }

  /// A tile on its way may be painted outside the group.
  Widget _group(Rect rect, List<Widget> children) => Stack(
    clipBehavior: Clip.none,
    children: [
      Positioned.fromRect(
        rect: rect,
        child: Stack(
          key: ShellKeys.tiles,
          clipBehavior: Clip.none,
          children: children,
        ),
      ),
    ],
  );
}

/// Focus mode's row of the others, centred, scrolling sideways when they don't
/// fit. It clips only sideways: a tile on its way comes from above it.
class _Strip extends StatelessWidget {
  const _Strip({required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) => ClipRect(
        clipper: const _SidewaysClip(),
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          clipBehavior: Clip.none,
          child: ConstrainedBox(
            constraints: BoxConstraints(minWidth: constraints.maxWidth),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              spacing: tileGap,
              children: children,
            ),
          ),
        ),
      ),
    );
  }
}

class _SidewaysClip extends CustomClipper<Rect> {
  const _SidewaysClip();

  /// Further than a window is tall.
  static const _reach = 1e5;

  @override
  Rect getClip(Size size) =>
      Rect.fromLTRB(0, -_reach, size.width, size.height + _reach);

  @override
  bool shouldReclip(_SidewaysClip oldClipper) => false;
}

/// Everyone without video hidden, and nobody has any.
class _NoVideo extends ConsumerWidget {
  const _NoVideo();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            'No one has video on',
            style: OcText.body.copyWith(color: colors.textSecondary),
          ),
          const SizedBox(height: OcSpace.s12),
          OcButton(
            label: 'Show everyone',
            onPressed: () =>
                ref.read(voiceViewPrefsProvider.notifier).toggleHideNonVideo(),
          ),
        ],
      ),
    );
  }
}

/// Before joining (§4.10): the channel, who is in it, and Join voice.
class _JoinPrompt extends ConsumerStatefulWidget {
  const _JoinPrompt({required this.channel, required this.participants});

  static const _shownAvatars = 8;

  final ChannelRef channel;
  final List<VoiceParticipant> participants;

  @override
  ConsumerState<_JoinPrompt> createState() => _JoinPromptState();
}

class _JoinPromptState extends ConsumerState<_JoinPrompt>
    with TickerProviderStateMixin {
  late final AnimationController _translateController;
  late final AnimationController _opacityController;

  @override
  void initState() {
    super.initState();
    _translateController = AnimationController.unbounded(
      vsync: this,
      value: 6.0,
    );
    _opacityController = AnimationController(vsync: this, value: 0.0);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (MediaQuery.disableAnimationsOf(context)) return;
      _translateController.animateWith(OcSpring.pop.simulation(6.0, 0.0, 0.0));
      _opacityController.animateTo(
        1.0,
        duration: const Duration(milliseconds: 100),
        curve: Curves.easeOut,
      );
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.disableAnimationsOf(context)) {
      _translateController.value = 0.0;
      _opacityController.value = 1.0;
    }
  }

  @override
  void dispose() {
    _translateController.dispose();
    _opacityController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final channel = widget.channel;
    final participants = widget.participants;
    final data = ref.watch(
      serverProvider(channel.server).select((state) => state.data),
    );
    final name = data?.channels[channel.channel]?.name ?? '';
    final canJoin =
        data?.permissionsIn(channel.channel).has(Permissions.connect) ?? false;
    final extra = participants.length - _JoinPrompt._shownAvatars;

    return AnimatedBuilder(
      animation: Listenable.merge([_translateController, _opacityController]),
      builder: (context, child) => Transform.translate(
        key: const ValueKey('join-prompt-motion'),
        offset: Offset(0, _translateController.value),
        child: Opacity(
          opacity: _opacityController.value.clamp(0.0, 1.0),
          child: child,
        ),
      ),
      child: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(OcSpace.s24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 64,
                height: 64,
                decoration: BoxDecoration(
                  color: colors.selected,
                  borderRadius: BorderRadius.circular(20),
                ),
                child: Center(
                  child: Icon(
                    OcIcons.volumeUp,
                    size: 20,
                    color: colors.textSecondary,
                  ),
                ),
              ),
              const SizedBox(height: 12),
              Text(
                name,
                textAlign: TextAlign.center,
                style: OcText.title.copyWith(color: colors.text),
              ),
              const SizedBox(height: 6),
              Text(
                participants.isEmpty
                    ? 'No one is here yet.'
                    : '${participants.length} connected',
                style: OcText.body.copyWith(
                  fontSize: 12.5,
                  color: colors.textSecondary,
                ),
              ),
              if (participants.isNotEmpty) ...[
                const SizedBox(height: 16),
                // Overlapping, like a stack of faces.
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    for (final participant in participants.take(
                      _JoinPrompt._shownAvatars,
                    ))
                      Align(
                        widthFactor: 0.8,
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            border: Border.all(color: colors.chat, width: 3),
                          ),
                          child: MemberAvatar(
                            serverKey: channel.server,
                            userId: participant.userId,
                            name:
                                data
                                    ?.members[participant.userId]
                                    ?.displayName ??
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
              const SizedBox(height: 16),
              OcButton.primary(
                label: 'Join voice',
                height: 36,
                padding: const EdgeInsets.symmetric(horizontal: 18),
                onPressed: canJoin
                    ? () => joinVoice(
                        context,
                        ref,
                        channel.server,
                        channel.channel,
                      )
                    : null,
              ),
              if (!canJoin) ...[
                const SizedBox(height: 8),
                Text(
                  "You don't have permission to join this channel.",
                  style: OcText.small.copyWith(color: colors.textMuted),
                ),
              ],
            ],
          ),
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
