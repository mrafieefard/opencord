import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';

/// The native window title (§3.1): `(3) #general · Opencord Dev — Opencord`,
/// with the unread prefix only when something is unread.
final windowTitleProvider = Provider<String>((ref) {
  final unread = ref.watch(unreadTotalProvider);
  final prefix = unread > 0 ? '($unread) ' : '';
  final server = ref.watch(currentServerProvider);
  if (server == null) return '${prefix}Opencord';
  final data = ref.watch(serverProvider(server).select((state) => state.data));
  final channel = data?.channels[ref.watch(currentChannelProvider)];
  final serverName =
      data?.info.name ??
      ref
          .watch(serverListProvider)
          .where((summary) => summary.key == server)
          .firstOrNull
          ?.name ??
      server;
  final place = [
    if (channel != null)
      channel.kind.isTextLike ? '#${channel.name}' : channel.name,
    serverName,
  ].join(' · ');
  return '$prefix$place — Opencord';
});
