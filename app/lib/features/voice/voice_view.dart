import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/user.dart';
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
class _Stage extends ConsumerWidget {
  const _Stage({required this.channel, required this.participants});

  static const _gap = OcSpace.s8;
  static const _strip = Size(160, 90);

  final ChannelRef channel;
  final List<VoiceParticipant> participants;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
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

    Widget tile(VoiceTileId id, Size size, {bool small = false}) {
      final participant = byUser[id.userId];
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
          speaking: speaking.contains(id.userId),
          focused: id == focused,
          small: small,
          onTap: () => ref.read(focus.notifier).toggle(id),
        ),
      );
    }

    return Padding(
      padding: const EdgeInsets.all(OcSpace.s16),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final area = constraints.biggest;
          if (focused != null) {
            final others = [
              for (final id in tiles)
                if (id != focused) id,
            ];
            final stripHeight = others.isEmpty ? 0.0 : _strip.height + _gap;
            final large = fitTiles(
              Size(area.width, math.max(0, area.height - stripHeight)),
              1,
            );
            return Column(
              children: [
                Expanded(child: Center(child: tile(focused, large))),
                if (others.isNotEmpty) ...[
                  const SizedBox(height: _gap),
                  SizedBox(
                    height: _strip.height,
                    child: Center(
                      child: SingleChildScrollView(
                        scrollDirection: Axis.horizontal,
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            for (final (index, id) in others.indexed) ...[
                              if (index > 0) const SizedBox(width: _gap),
                              tile(id, _strip, small: true),
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
          final size = fitTiles(area, tiles.length, gap: _gap);
          final columns = gridColumns(tiles.length);
          final rows = [
            for (var start = 0; start < tiles.length; start += columns)
              tiles.sublist(start, math.min(start + columns, tiles.length)),
          ];
          return Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              for (final (index, row) in rows.indexed) ...[
                if (index > 0) const SizedBox(height: _gap),
                Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    for (final (column, id) in row.indexed) ...[
                      if (column > 0) const SizedBox(width: _gap),
                      tile(id, size),
                    ],
                  ],
                ),
              ],
            ],
          );
        },
      ),
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
