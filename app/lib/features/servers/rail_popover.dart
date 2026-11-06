import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

/// Shows up to 4 participant avatars and a count for a server's voice channels (§5.2).
class VoiceFaces extends StatelessWidget {
  const VoiceFaces({
    super.key,
    required this.serverKey,
    required this.userIds,
    required this.names,
  });

  final String serverKey;
  final List<int> userIds;
  final Map<int, String> names;

  @override
  Widget build(BuildContext context) {
    if (userIds.isEmpty) return const SizedBox.shrink();
    final colors = context.oc;
    final count = userIds.length;
    final shown = userIds.take(4).toList();
    final more = count > 4 ? count - 4 : 0;
    final totalItems = shown.length + (more > 0 ? 1 : 0);
    final stackWidth = 26.0 + (totalItems - 1) * 20.0;

    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Container(
        padding: const EdgeInsets.only(top: 8),
        decoration: BoxDecoration(
          border: Border(top: BorderSide(color: colors.border, width: 1)),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            SizedBox(
              width: stackWidth,
              height: 26,
              child: Stack(
                clipBehavior: Clip.none,
                children: [
                  for (var i = 0; i < shown.length; i++)
                    Positioned(
                      left: i * 20.0,
                      top: 0,
                      child: Container(
                        width: 26,
                        height: 26,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          boxShadow: [
                            BoxShadow(color: colors.elevated, spreadRadius: 2),
                          ],
                        ),
                        child: MemberAvatar(
                          serverKey: serverKey,
                          userId: shown[i],
                          name: names[shown[i]] ?? '',
                          size: 26,
                        ),
                      ),
                    ),
                  if (more > 0)
                    Positioned(
                      left: shown.length * 20.0,
                      top: 0,
                      child: Container(
                        width: 26,
                        height: 26,
                        decoration: BoxDecoration(
                          color: colors.selected,
                          shape: BoxShape.circle,
                          boxShadow: [
                            BoxShadow(color: colors.elevated, spreadRadius: 2),
                          ],
                        ),
                        alignment: Alignment.center,
                        child: Text(
                          '+$more',
                          textScaler: TextScaler.noScaling,
                          style: OcText.bodyStrong.copyWith(
                            fontSize: 10.5,
                            height: 1,
                            color: colors.text,
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
            const SizedBox(width: 10),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(OcIcons.volumeUp, size: 16, color: colors.textSecondary),
                const SizedBox(width: 4),
                Text(
                  '$count in voice',
                  style: OcText.body.copyWith(
                    fontSize: 12,
                    color: colors.textSecondary,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// The rail's popover (§5.2): replaces tooltips with server info and voice participants.
class RailPopover extends ConsumerWidget {
  const RailPopover({super.key, required this.id});

  final Object id;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;

    final String name;
    final String? subtitle;
    final List<int> voiceUserIds;
    final Map<int, String> voiceNames;
    final String? serverKey;

    if (id == 'add') {
      name = 'Add a server';
      subtitle = null;
      voiceUserIds = const [];
      voiceNames = const {};
      serverKey = null;
    } else if (id == 'private' || id == 'private-home') {
      name = 'Private messages';
      final contacts = ref.watch(privateContactsProvider);
      final n = contacts.incomingRequests.length;
      final m = contacts.messageRequests.length;
      final friends = n == 1 ? '1 friend request' : '$n friend requests';
      final msgs = m == 1 ? '1 message request' : '$m message requests';
      subtitle = '$friends · $msgs';
      voiceUserIds = const [];
      voiceNames = const {};
      serverKey = null;
    } else if (id is String) {
      serverKey = id as String;
      final state = ref.watch(serverProvider(serverKey));
      final serverList = ref.watch(serverListProvider);
      final exists = serverList.any((s) => s.key == serverKey);
      if (!exists && state.data == null) {
        return const SizedBox.shrink();
      }

      name =
          state.data?.info.name ??
          serverList.where((s) => s.key == serverKey).firstOrNull?.name ??
          serverKey;
      final online = ref.watch(
        presenceProvider(serverKey).select(
          (p) => p.presences.values
              .where((presence) => presence != Presence.offline)
              .length,
        ),
      );
      final members = state.data?.members.length ?? 0;
      subtitle = switch (state.connection.phase) {
        ConnectionPhase.connected =>
          '${countLabel(online)} online · ${countLabel(members)} members',
        ConnectionPhase.connecting => 'Connecting…',
        ConnectionPhase.reconnecting => 'Reconnecting…',
        ConnectionPhase.failed => 'Connection failed',
      };

      final voiceMap = ref.watch(voiceProvider(serverKey));
      final participants = voiceMap.values.expand((p) => p).toList();
      voiceUserIds = participants.map((p) => p.userId).toSet().toList();
      final data = state.data;
      voiceNames = <int, String>{};
      for (final uid in voiceUserIds) {
        if (data?.self.id == uid) {
          voiceNames[uid] = data?.self.displayName ?? 'You';
        } else {
          final m = data?.members[uid];
          voiceNames[uid] = m?.displayName ?? m?.user.displayName ?? 'Someone';
        }
      }
    } else {
      return const SizedBox.shrink();
    }

    return IgnorePointer(
      child: Stack(
        clipBehavior: Clip.none,
        alignment: Alignment.centerLeft,
        children: [
          Container(
            key: ShellKeys.railPop,
            constraints: const BoxConstraints(minWidth: 168, maxWidth: 280),
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            decoration: BoxDecoration(
              color: colors.elevated,
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: colors.border, width: 1),
              boxShadow: OcShadows.menu(colors),
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: OcText.bodyStrong.copyWith(
                    fontSize: 13.5,
                    color: colors.text,
                  ),
                ),
                if (subtitle != null)
                  Text(
                    subtitle,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.body.copyWith(
                      fontSize: 12,
                      color: colors.textMuted,
                    ),
                  ),
                if (voiceUserIds.isNotEmpty && serverKey != null)
                  VoiceFaces(
                    serverKey: serverKey,
                    userIds: voiceUserIds,
                    names: voiceNames,
                  ),
              ],
            ),
          ),
          Positioned(
            left: -4,
            top: 0,
            bottom: 0,
            child: Center(
              child: Transform.rotate(
                key: const Key('rail-popover-arrow'),
                angle: math.pi / 4,
                child: Container(
                  width: 8,
                  height: 8,
                  decoration: BoxDecoration(
                    color: colors.elevated,
                    border: Border(
                      left: BorderSide(color: colors.border, width: 1),
                      bottom: BorderSide(color: colors.border, width: 1),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
