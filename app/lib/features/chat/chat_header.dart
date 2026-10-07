import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/chat/pinned_bar.dart';
import 'package:opencord/features/chat/typing_dots.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/toast.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';

/// The open channel's header (§4.3): its name, a subtitle with the member
/// counts and topic that turns into "Kai is typing" while someone types,
/// and the channel's buttons.
class ChatHeader extends ConsumerWidget {
  const ChatHeader({
    super.key,
    required this.channel,
    required this.membersShown,
    required this.onToggleMembers,
    required this.onJumpTo,
    this.onOpenSidebar,
    this.leadingControls = false,
    this.trailingControls = false,
  });

  /// Null while no channel is open.
  final ChannelRef? channel;
  final bool membersShown;
  final VoidCallback onToggleMembers;
  final ValueChanged<int> onJumpTo;
  final VoidCallback? onOpenSidebar;
  final bool leadingControls;
  final bool trailingControls;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final open = channel;
    final target = open == null
        ? null
        : ref.watch(
            serverProvider(
              open.server,
            ).select((state) => state.data?.channels[open.channel]),
          );
    final pins = ref.read(repositoryProvider).capabilities.pins;
    return HeaderBar(
      leadingControls: leadingControls,
      trailingControls: trailingControls,
      child: Row(
        children: [
          if (onOpenSidebar != null)
            OcIconButton(
              icon: OcIcons.menu,
              tooltip: 'Channels',
              onPressed: onOpenSidebar,
            ),
          const SizedBox(width: OcSpace.s4),
          Expanded(
            child: IgnorePointer(
              child: open == null || target == null
                  ? const SizedBox()
                  : _Title(channel: open, target: target),
            ),
          ),
          if (open != null && target != null && target.kind.isTextLike) ...[
            OcIconButton(
              icon: OcIcons.search,
              tooltip: 'Search in channel',
              onPressed: () =>
                  showOcToast(context, 'Search is coming in a later version.'),
            ),
            if (pins)
              Builder(
                builder: (context) => OcIconButton(
                  icon: OcIcons.pushPin,
                  tooltip: 'Pinned messages',
                  onPressed: () => showPopover<void>(
                    context: context,
                    anchor: globalRectOf(context),
                    alignEnd: true,
                    padding: const EdgeInsets.all(OcSpace.s8),
                    builder: (popover) => PinnedList(
                      channel: open,
                      onJumpTo: (id) {
                        Navigator.pop(popover);
                        onJumpTo(id);
                      },
                    ),
                  ),
                ),
              ),
          ],
          OcIconButton(
            icon: OcIcons.group,
            tooltip: 'Member list',
            active: membersShown,
            onPressed: onToggleMembers,
          ),
          if (open != null && target != null)
            Builder(
              builder: (context) => OcIconButton(
                icon: OcIcons.moreVert,
                tooltip: 'More',
                onPressed: () => _showMore(context, ref, open),
              ),
            ),
        ],
      ),
    );
  }

  /// §4.3's More menu. Channel settings joins it with the settings pages
  /// (step 11); notification settings are mute and unmute for now.
  void _showMore(BuildContext context, WidgetRef ref, ChannelRef channel) {
    final muted = ref
        .read(notificationPrefsProvider)
        .channelMuted(channel.server, channel.channel);
    final rect = globalRectOf(context);
    showOcMenu(
      context: context,
      position: Offset(rect.right, rect.bottom + OcSpace.s4),
      entries: [
        OcMenuItem(
          label: muted ? 'Unmute channel' : 'Mute channel',
          icon: muted ? OcIcons.notifications : OcIcons.notificationsOff,
          onSelected: () => ref
              .read(notificationPrefsProvider.notifier)
              .toggleChannel(channel.server, channel.channel),
        ),
        OcMenuItem(
          label: 'Copy link',
          icon: OcIcons.link,
          onSelected: () {
            Clipboard.setData(
              ClipboardData(text: channelLink(channel.server, channel.channel)),
            );
            showOcToast(context, 'Link copied');
          },
        ),
        OcMenuItem(
          label: 'Mark as read',
          icon: OcIcons.markChatRead,
          onSelected: () => ref
              .read(activityProvider(channel.server).notifier)
              .markRead(channel.channel),
        ),
      ],
    );
  }
}

class _Title extends ConsumerWidget {
  const _Title({required this.channel, required this.target});

  final ChannelRef channel;
  final Channel target;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final typing = ref.watch(
      typingProvider(channel.server).select((t) => t.users(channel.channel)),
    );
    final members = ref.watch(
      serverProvider(channel.server).select((state) => state.data?.members),
    );
    final names = [
      for (final id in typing) members?[id]?.displayName ?? 'Someone',
    ];
    final audience = ref.watch(channelAudienceProvider(channel));
    // Voice channels count who is in them instead (§4.10).
    final connected = target.kind == ChannelKind.voice
        ? ref.watch(
            voiceProvider(
              channel.server,
            ).select((voice) => voice[channel.channel]?.length ?? 0),
          )
        : null;
    final topic = target.topic;
    final style = OcText.small.copyWith(color: colors.textSecondary);
    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        EllipsisText(
          target.kind.isTextLike ? '#${target.name}' : target.name,
          style: OcText.header.copyWith(color: colors.text),
        ),
        if (names.isNotEmpty)
          Row(
            children: [
              Flexible(
                child: Text(
                  typingLabel(names),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: style,
                ),
              ),
              TypingDots(color: colors.textSecondary),
            ],
          )
        else
          Text(
            switch (connected) {
              0 => 'No one connected',
              final count? => '${countLabel(count)} connected',
              null => [
                '${countLabel(audience.members)} member'
                    '${audience.members == 1 ? '' : 's'}, '
                    '${countLabel(audience.online)} online',
                if (topic != null && topic.isNotEmpty) topic,
              ].join(' · '),
            },
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: style,
          ),
      ],
    );
  }
}
