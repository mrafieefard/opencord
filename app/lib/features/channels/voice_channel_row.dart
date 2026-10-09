import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/voice/voice_focus.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';

/// A voice channel (§4.2): one line, then who is in it.
class VoiceChannelRow extends ConsumerWidget {
  const VoiceChannelRow({
    super.key,
    required this.serverKey,
    required this.channel,
  });

  final String serverKey;
  final Channel channel;

  Future<void> _join(BuildContext context, WidgetRef ref) async {
    ref.read(navigationProvider.notifier).openChannel(serverKey, channel.id);
    await joinVoice(context, ref, serverKey, channel.id);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final participants = ref.watch(
      voiceProvider(
        serverKey,
      ).select((voice) => voice[channel.id] ?? const <VoiceParticipant>[]),
    );
    final selected = ref.watch(currentChannelProvider) == channel.id;
    // Phase 1 servers have no voice: the channel shows, but only opens a
    // note that it is coming (Phase 1 §9.3).
    final voice = ref.read(repositoryProvider).capabilities.voice;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Hoverable(
          onTap: voice
              ? () => _join(context, ref)
              : () => ref
                    .read(navigationProvider.notifier)
                    .openChannel(serverKey, channel.id),
          onSecondaryTap: (position) => _showMenu(context, ref, position),
          semanticLabel: voice
              ? '${channel.name}, voice, ${participants.length} connected'
              : '${channel.name}, voice, coming soon',
          selected: selected,
          cursor: SystemMouseCursors.basic,
          builder: (context, state) => AnimatedContainer(
            duration: OcMotion.of(context).hover,
            height: 36,
            margin: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s10),
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
                  OcIcons.volumeUp,
                  size: OcSize.iconRow,
                  color: selected ? colors.text : colors.textSecondary,
                ),
                const SizedBox(width: OcSpace.s8),
                Expanded(
                  child: EllipsisText(
                    channel.name,
                    style: OcText.body.copyWith(
                      fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                      color: selected ? colors.text : colors.textSecondary,
                    ),
                  ),
                ),
                if (!voice)
                  Text(
                    'Soon',
                    style: OcText.small.copyWith(color: colors.textMuted),
                  )
                else if (participants.isNotEmpty)
                  Text(
                    '${participants.length}',
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
              ],
            ),
          ),
        ),
        for (final participant in participants)
          _Participant(
            serverKey: serverKey,
            channelId: channel.id,
            participant: participant,
          ),
      ],
    );
  }

  Future<void> _showMenu(BuildContext context, WidgetRef ref, Offset position) {
    final manage =
        ref
            .read(serverProvider(serverKey))
            .data
            ?.can(Permissions.manageChannels) ??
        false;
    return showOcMenu(
      context: context,
      position: position,
      entries: [
        if (ref.read(repositoryProvider).capabilities.voice)
          OcMenuItem(
            label: 'Join',
            icon: OcIcons.volumeUp,
            onSelected: () => _join(context, ref),
          ),
        OcMenuItem(
          label: 'Open in view',
          icon: OcIcons.openInFull,
          onSelected: () => ref
              .read(navigationProvider.notifier)
              .openChannel(serverKey, channel.id),
        ),
        OcMenuItem(
          label: 'Copy link',
          icon: OcIcons.link,
          onSelected: () {
            Clipboard.setData(
              ClipboardData(text: channelLink(serverKey, channel.id)),
            );
            showOcToast(context, 'Link copied');
          },
        ),
        if (manage) ...[
          const OcMenuDivider(),
          OcMenuItem(
            label: 'Edit channel',
            icon: OcIcons.edit,
            onSelected: () => showServerSettings(
              context,
              ref,
              serverKey: serverKey,
              page: 'channels',
              channel: channel.id,
            ),
          ),
          OcMenuItem(
            label: 'Delete channel',
            icon: OcIcons.delete,
            onSelected: () =>
                confirmDeleteChannel(context, ref, serverKey, channel),
          ),
        ],
      ],
    );
  }
}

class _Participant extends ConsumerWidget {
  const _Participant({
    required this.serverKey,
    required this.channelId,
    required this.participant,
  });

  final String serverKey;
  final int channelId;
  final VoiceParticipant participant;

  /// Clicking LIVE joins the channel if needed and watches the stream
  /// (§9.5).
  Future<void> _watch(BuildContext context, WidgetRef ref) async {
    ref.read(navigationProvider.notifier).openChannel(serverKey, channelId);
    final voice = ref.read(voiceSessionProvider);
    if (voice.serverKey != serverKey || voice.channelId != channelId) {
      await joinVoice(context, ref, serverKey, channelId);
    }
    if (!context.mounted) return;
    final stream = ref
        .read(streamsProvider(serverKey))
        .values
        .where(
          (stream) =>
              stream.userId == participant.userId &&
              stream.channelId == channelId,
        )
        .firstOrNull;
    final self = ref.read(serverProvider(serverKey)).data?.self.id;
    if (stream == null || participant.userId == self) return;
    await watchStream(
      context,
      ref,
      stream.key,
      opened: () => ref
          .read(
            voiceFocusProvider((
              server: serverKey,
              channel: channelId,
            )).notifier,
          )
          .focus((userId: participant.userId, screen: true)),
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final member = ref.watch(
      serverProvider(
        serverKey,
      ).select((s) => s.data?.members[participant.userId]),
    );
    final speaking = ref.watch(
      speakingProvider(serverKey).select((s) => s.contains(participant.userId)),
    );
    final name = member?.displayName ?? 'Someone';
    final state = [
      if (participant.muted) 'muted',
      if (participant.deafened) 'deafened',
      if (participant.camera) 'camera on',
      if (participant.screensharing) 'sharing their screen',
      if (speaking) 'speaking',
    ];
    return Semantics(
      label: state.isEmpty ? name : '$name, ${state.join(', ')}',
      child: Padding(
        padding: const EdgeInsets.fromLTRB(44, 2, 16, 2),
        child: SizedBox(
          height: 28,
          child: Row(
            children: [
              OcAvatar(
                id: participant.userId,
                name: name,
                size: OcSize.voiceAvatar,
                speaking: speaking,
              ),
              const SizedBox(width: OcSpace.s8),
              Expanded(
                child: EllipsisText(
                  name,
                  style: OcText.small.copyWith(
                    color: speaking ? colors.text : colors.textSecondary,
                  ),
                ),
              ),
              for (final icon in [
                if (participant.muted) OcIcons.micOff,
                if (participant.deafened) OcIcons.headsetOff,
                if (participant.camera) OcIcons.videocam,
              ])
                Padding(
                  padding: const EdgeInsets.only(left: OcSpace.s4),
                  child: Icon(
                    icon,
                    size: OcSize.iconInline,
                    color: colors.textMuted,
                  ),
                ),
              if (participant.screensharing) ...[
                const SizedBox(width: OcSpace.s4),
                Hoverable(
                  onTap: () => _watch(context, ref),
                  semanticLabel: 'Watch $name',
                  focusRadius: BorderRadius.circular(8),
                  builder: (context, state) => const LivePill(),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
