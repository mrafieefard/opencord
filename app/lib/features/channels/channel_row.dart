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
import 'package:opencord/features/chat/attachments/summary.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';

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
    final isPlugin = channel.kind == ChannelKind.plugin;
    final unavailableReason = cannotOpenReason(channel);
    final unavailable = unavailableReason != null;

    final channelType = isPlugin
        ? data.pluginChannelTypes
              .where(
                (t) =>
                    t.pluginId == channel.pluginId &&
                    t.typeKey == channel.typeKey,
              )
              .firstOrNull
        : null;
    final iconPath = channelType?.icon ?? '';
    final iconFile = iconPath.isNotEmpty
        ? ref
              .watch(
                pluginAssetFileProvider(
                  PluginAssetRef(
                    serverKey: serverKey,
                    pluginId: channel.pluginId ?? '',
                    path: iconPath,
                  ),
                ),
              )
              .value
        : null;

    final last = activity.last;
    final now = ref.watch(clockProvider)();
    String preview() {
      if (isPlugin) {
        if (channel.topic case final topic? when topic.isNotEmpty) return topic;
        return channelType?.description ?? '';
      }
      if (last == null) return 'No messages yet';
      final author = last.authorId == data.self.id
          ? 'You'
          : data.authorOf(last.authorId).name;
      return switch (last.systemEvent) {
        SystemEvent.memberJoined => '$author joined the server',
        SystemEvent.channelCreated => '$author created the channel',
        SystemEvent.messagePinned => '$author pinned a message',
        null =>
          '$author: ${previewText(messageSummary(last), user: (id) => data.authorOf(id).name, channel: (id) => data.channels[id]?.name)}',
      };
    }

    final locked = isPlugin
        ? !permissions.has(Permissions.viewChannel) ||
              isPrivate(channel, data.info.everyoneRoleId)
        : !permissions.has(Permissions.sendMessages) ||
              isPrivate(channel, data.info.everyoneRoleId) ||
              isReadOnly(channel, data.info.everyoneRoleId);

    return Hoverable(
      onTap: () {
        if (canOpen(context, channel)) {
          ref
              .read(navigationProvider.notifier)
              .openChannel(serverKey, channel.id);
        }
      },
      onSecondaryTap: (position) => _showMenu(context, ref, position, muted),
      semanticLabel: '${channel.name}, ${activity.read.unread} unread',
      selected: selected,
      cursor: SystemMouseCursors.basic,
      builder: (context, state) {
        final rowView = ChannelRowView(
          kind: channel.kind,
          name: channel.name,
          time: isPlugin || last == null ? null : rowTime(last.createdAt, now),
          preview: preview(),
          unread: isPlugin ? 0 : activity.read.unread,
          mentions: isPlugin ? 0 : activity.read.mentions,
          muted: muted,
          locked: locked,
          selected: selected,
          hovered: state.active,
          unavailable: unavailable,
          iconFile: iconFile,
        );
        if (unavailableReason != null) {
          return Tooltip(message: unavailableReason, child: rowView);
        }
        return rowView;
      },
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
    final isPlugin = channel.kind == ChannelKind.plugin;
    return showOcMenu(
      context: context,
      position: position,
      entries: [
        if (!isPlugin) ...[
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
        ],
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

/// Asks before deleting a channel for everyone (§4.11, §16).
/// True once it is deleted. A category's channels stay, outside any
/// category.
Future<bool> confirmDeleteChannel(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  Channel channel,
) async {
  final name = channel.kind.isTextLike ? '#${channel.name}' : channel.name;
  final delete = channel.isCategory
      ? await confirmAction(
          context,
          title: 'Delete the ${channel.name} category?',
          message: 'Its channels stay, outside any category.',
          action: 'Delete category',
        )
      : await confirmAction(
          context,
          title: 'Delete $name?',
          message: 'This deletes $name and its messages for everyone.',
          action: 'Delete channel',
        );
  if (!delete) return false;
  try {
    await ref.read(repositoryProvider).deleteChannel(serverKey, channel.id);
    return true;
  } on RepoException catch (error) {
    if (context.mounted) showOcToast(context, error.message);
    return false;
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
    this.unavailable = false,
    this.iconFile,
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
  final bool unavailable;
  final String? iconFile;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final background = selected
        ? colors.selected
        : hovered
        ? colors.hover
        : colors.sidebar;
    final strong = !unavailable && (unread > 0 || selected);
    return AnimatedContainer(
      duration: OcMotion.of(context).hover,
      height: 56,
      margin: const EdgeInsets.fromLTRB(8, 0, 8, 2),
      padding: const EdgeInsets.symmetric(horizontal: 8),
      decoration: BoxDecoration(
        color: background,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        children: [
          ChannelGlyph(
            kind: kind,
            selected: selected,
            locked: locked,
            ringColor: background,
            iconFile: iconFile,
            unavailable: unavailable,
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: EllipsisText(
                        name,
                        style: OcText.body.copyWith(
                          fontSize: 14,
                          fontWeight: strong
                              ? FontWeight.w600
                              : FontWeight.w500,
                          color: unavailable
                              ? colors.textMuted
                              : strong
                              ? colors.text
                              : colors.textSecondary,
                        ),
                      ),
                    ),
                    if (time != null) ...[
                      const SizedBox(width: 8),
                      Text(
                        time!,
                        style: OcText.meta.copyWith(
                          fontSize: 11,
                          color: colors.textMuted,
                        ),
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
                        style: OcText.small.copyWith(
                          fontSize: 12.5,
                          color: colors.textMuted,
                        ),
                      ),
                    ),
                    if (mentions > 0) ...[
                      const SizedBox(width: 8),
                      const MentionBadge(),
                    ],
                    if (unread > 0) ...[
                      SizedBox(width: mentions > 0 ? 4 : 8),
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
