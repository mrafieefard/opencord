import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// `opencord://host:port/c/123`: what "Copy link" puts on the clipboard.
String channelLink(String serverKey, int channelId) =>
    'opencord://$serverKey/c/$channelId';

/// A channel only some roles or members can see.
bool isPrivate(Channel channel, int everyoneRoleId) =>
    _everyoneDenies(channel, everyoneRoleId, Permissions.viewChannel);

/// A channel most members can read but not write in.
bool isReadOnly(Channel channel, int everyoneRoleId) =>
    _everyoneDenies(channel, everyoneRoleId, Permissions.sendMessages);

bool _everyoneDenies(Channel channel, int everyoneRoleId, Permissions denied) =>
    channel.overwrites.any(
      (overwrite) =>
          overwrite.targets(OverwriteTargetKind.role, everyoneRoleId) &&
          overwrite.deny.has(denied),
    );

/// A text or announcement channel row (§4.2): two lines, Telegram style.
class ChannelRow extends ConsumerWidget {
  const ChannelRow({super.key, required this.serverKey, required this.channel});

  final String serverKey;
  final Channel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final data = ref.watch(serverProvider(serverKey).select((s) => s.data));
    if (data == null) return const SizedBox.shrink();
    final activity = ref.watch(
      activityProvider(serverKey).select((a) => a.of(channel.id)),
    );
    final muted = ref.watch(
      notificationPrefsProvider.select((p) => p.muted(serverKey, channel.id)),
    );
    final selected = ref.watch(currentChannelProvider) == channel.id;
    final permissions = data.permissionsIn(channel.id);
    final last = activity.last;
    final now = ref.watch(clockProvider)();
    String preview() {
      if (last == null) return 'No messages yet';
      final author = last.authorId == data.self.id
          ? 'You'
          : data.members[last.authorId]?.displayName ?? 'Someone';
      return switch (last.systemEvent) {
        SystemEvent.memberJoined => '$author joined the server',
        SystemEvent.channelCreated => '$author created the channel',
        SystemEvent.messagePinned => '$author pinned a message',
        null =>
          '$author: ${previewText(last.content, user: (id) => data.members[id]?.displayName, channel: (id) => data.channels[id]?.name)}',
      };
    }

    return Hoverable(
      onTap: () => ref
          .read(navigationProvider.notifier)
          .openChannel(serverKey, channel.id),
      onSecondaryTap: (position) => _showMenu(context, ref, position, muted),
      semanticLabel: '${channel.name}, ${activity.read.unread} unread',
      selected: selected,
      cursor: SystemMouseCursors.basic,
      builder: (context, state) => ChannelRowView(
        kind: channel.kind,
        name: channel.name,
        time: last == null ? null : rowTime(last.createdAt, now),
        preview: preview(),
        unread: activity.read.unread,
        mentions: activity.read.mentions,
        muted: muted,
        // The lock describes the channel, not only what this user may do
        // there (§4.2): private, read-only for most, or read-only for them.
        locked:
            !permissions.has(Permissions.sendMessages) ||
            isPrivate(channel, data.info.everyoneRoleId) ||
            isReadOnly(channel, data.info.everyoneRoleId),
        selected: selected,
        hovered: state.active,
      ),
    );
  }

  Future<void> _showMenu(
    BuildContext context,
    WidgetRef ref,
    Offset position,
    bool muted,
  ) {
    final data = ref.read(serverProvider(serverKey)).data;
    final manage = data?.can(Permissions.manageChannels) ?? false;
    return showOcMenu(
      context: context,
      position: position,
      entries: [
        OcMenuItem(
          label: 'Mark as read',
          icon: OcIcons.markChatRead,
          onSelected: () => ref
              .read(activityProvider(serverKey).notifier)
              .markRead(channel.id),
        ),
        OcMenuItem(
          label: muted ? 'Unmute channel' : 'Mute channel',
          icon: muted ? OcIcons.notifications : OcIcons.notificationsOff,
          onSelected: () => ref
              .read(notificationPrefsProvider.notifier)
              .toggleChannel(serverKey, channel.id),
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

/// Asks before deleting a channel for everyone (§4.11, §16).
Future<void> confirmDeleteChannel(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  Channel channel,
) async {
  final name = channel.kind.isTextLike ? '#${channel.name}' : channel.name;
  final delete = await confirmAction(
    context,
    title: 'Delete $name?',
    message: 'This deletes $name and its messages for everyone.',
    action: 'Delete channel',
  );
  if (!delete) return;
  try {
    await ref.read(repositoryProvider).deleteChannel(serverKey, channel.id);
  } on RepoException catch (error) {
    if (context.mounted) showOcToast(context, error.message);
  }
}

/// The look of a channel row in each of its states, apart from its data.
class ChannelRowView extends StatelessWidget {
  const ChannelRowView({
    super.key,
    required this.kind,
    required this.name,
    required this.preview,
    this.time,
    this.unread = 0,
    this.mentions = 0,
    this.muted = false,
    this.locked = false,
    this.selected = false,
    this.hovered = false,
  });

  final ChannelKind kind;
  final String name;
  final String preview;
  final String? time;
  final int unread;
  final int mentions;
  final bool muted;
  final bool locked;
  final bool selected;
  final bool hovered;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final background = selected
        ? colors.selected
        : hovered
        ? colors.hover
        : colors.sidebar;
    final strong = unread > 0 || selected;
    return AnimatedContainer(
      duration: OcMotion.of(context).hover,
      height: OcSize.channelRow,
      margin: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
      padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
      decoration: BoxDecoration(
        color: background,
        borderRadius: BorderRadius.circular(OcRadius.row),
      ),
      child: Row(
        children: [
          ChannelGlyph(
            kind: kind,
            selected: selected,
            locked: locked,
            ringColor: background,
          ),
          const SizedBox(width: OcSpace.s10),
          Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.body.copyWith(
                          fontWeight: strong
                              ? FontWeight.w600
                              : FontWeight.w500,
                          color: strong ? colors.text : colors.textSecondary,
                        ),
                      ),
                    ),
                    if (time != null) ...[
                      const SizedBox(width: OcSpace.s6),
                      Text(
                        time!,
                        style: OcText.meta.copyWith(color: colors.textMuted),
                      ),
                    ],
                  ],
                ),
                const SizedBox(height: 2),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        preview,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.small.copyWith(color: colors.textMuted),
                      ),
                    ),
                    if (mentions > 0) ...[
                      const SizedBox(width: OcSpace.s6),
                      const MentionBadge(),
                    ],
                    if (unread > 0) ...[
                      const SizedBox(width: OcSpace.s4),
                      UnreadBadge(count: unread, muted: muted),
                    ],
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
