import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';

class TitleCrumb extends ConsumerWidget {
  const TitleCrumb({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final homeShown = ref.watch(homeShownProvider);
    final servers = ref.watch(serverListProvider);
    final contacts = ref.watch(privateContactsProvider);

    if (servers.isEmpty && contacts.all.isEmpty) {
      return const SizedBox.shrink();
    }

    Widget leftIcon;
    String leftName;

    if (!homeShown) {
      final serverKey = ref.watch(currentServerProvider);
      final serverSummary = servers
          .where((s) => s.key == serverKey)
          .firstOrNull;
      final serverState = serverKey != null
          ? ref.watch(serverProvider(serverKey))
          : null;
      final serverName =
          serverState?.data?.info.name ?? serverSummary?.name ?? '';

      leftIcon = ServerAvatar(
        serverKey: serverKey ?? '',
        name: serverName,
        size: 20,
        borderRadius: 6,
        backgroundColor: colors.selected,
        initialsStyle: OcText.bodyStrong.copyWith(
          fontSize: 8.5,
          fontWeight: FontWeight.w700,
          color: colors.text,
          height: 1,
        ),
      );
      leftName = serverName;
    } else {
      leftIcon = Container(
        width: 20,
        height: 20,
        alignment: Alignment.center,
        decoration: ShapeDecoration(
          color: colors.selected,
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
        ),
        child: Icon(OcIcons.chat, size: 13, color: colors.text),
      );
      leftName = 'Private messages';
    }

    Widget rightIcon;
    String rightName;

    if (!homeShown) {
      final serverKey = ref.watch(currentServerProvider);
      final channelId = ref.watch(currentChannelProvider);
      final channel = (serverKey != null && channelId != null)
          ? ref.watch(serverProvider(serverKey)).data?.channels[channelId]
          : null;

      if (channel != null) {
        rightIcon = Icon(
          channelIcon(channel.kind),
          size: 16,
          color: colors.textSecondary,
        );
        rightName = channel.name.replaceFirst(RegExp(r'^#'), '');
      } else {
        rightIcon = Icon(OcIcons.tag, size: 16, color: colors.textSecondary);
        rightName = '';
      }
    } else {
      final peer = ref.watch(currentPeerProvider);
      if (peer != null) {
        final name = ref.watch(contactNameProvider(peer));
        final avatar = ref.watch(contactAvatarProvider(peer));
        rightIcon = OcAvatar(
          id: peer,
          name: name,
          size: 20,
          image: avatar != null
              ? avatarImage(ref, avatar.serverKey, null)
              : null,
        );
        rightName = name;
      } else {
        final tab = ref.watch(friendsTabProvider);
        if (tab == FriendsTab.requests) {
          rightIcon = Icon(OcIcons.mail, size: 16, color: colors.textSecondary);
          rightName = 'Message requests';
        } else {
          rightIcon = Icon(
            OcIcons.group,
            size: 16,
            color: colors.textSecondary,
          );
          rightName = 'Friends';
        }
      }
    }

    return Row(
      key: ShellKeys.crumb,
      mainAxisSize: MainAxisSize.min,
      children: [
        leftIcon,
        const SizedBox(width: 7),
        Flexible(
          child: Text(
            leftName,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: OcText.body.copyWith(
              fontSize: 13,
              fontWeight: FontWeight.w500,
              color: colors.textSecondary,
              height: 1.25,
            ),
          ),
        ),
        const SizedBox(width: 7),
        Text(
          '/',
          style: OcText.body.copyWith(
            fontSize: 13,
            color: colors.textMuted,
            height: 1.25,
          ),
        ),
        const SizedBox(width: 7),
        rightIcon,
        const SizedBox(width: 7),
        Flexible(
          child: Text(
            rightName,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: OcText.bodyStrong.copyWith(
              fontSize: 13,
              fontWeight: FontWeight.w600,
              color: colors.text,
              height: 1.25,
            ),
          ),
        ),
      ],
    );
  }
}
