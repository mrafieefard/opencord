import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/key_value_store.dart';

/// A category (or the loose channels above all categories) with its
/// channels in display order.
@immutable
class ChannelGroup {
  const ChannelGroup(this.category, this.channels);

  /// Null for channels without a (visible) category.
  final Channel? category;
  final List<Channel> channels;
}

int _byPosition(Channel a, Channel b) {
  final order = a.position.compareTo(b.position);
  return order != 0 ? order : a.id.compareTo(b.id);
}

/// Channels as the sidebar shows them: loose channels first, then each
/// category; inside a group, text channels before voice channels.
List<ChannelGroup> channelTree(Iterable<Channel> channels) {
  final categories = channels.where((c) => c.isCategory).toList()
    ..sort(_byPosition);
  final categoryIds = {for (final category in categories) category.id};
  List<Channel> sorted(Iterable<Channel> group) => [
    ...group.where((c) => c.kind.isTextLike).toList()..sort(_byPosition),
    ...group.where((c) => c.kind == ChannelKind.voice).toList()
      ..sort(_byPosition),
  ];
  final loose = sorted(
    channels.where((c) => !c.isCategory && !categoryIds.contains(c.parentId)),
  );
  return [
    if (loose.isNotEmpty) ChannelGroup(null, loose),
    for (final category in categories)
      ChannelGroup(
        category,
        sorted(channels.where((c) => c.parentId == category.id)),
      ),
  ];
}

/// Text and announcement channels in sidebar order: what Alt+↑/↓ steps
/// through (§7).
List<Channel> navigableChannels(Iterable<Channel> channels) => [
  for (final group in channelTree(channels))
    ...group.channels.where((c) => c.kind.isTextLike),
];

/// The item [delta] steps from [current], wrapping around; the first item
/// when [current] is not in [order].
T? neighbor<T>(List<T> order, T? current, int delta) {
  if (order.isEmpty) return null;
  final index = order.indexOf(current as T);
  if (index == -1) return order.first;
  return order[(index + delta) % order.length];
}

/// The nearest item in direction [delta] that passes [test], wrapping
/// around and skipping [current] itself.
T? neighborWhere<T>(
  List<T> order,
  T? current,
  int delta,
  bool Function(T item) test,
) {
  if (order.isEmpty) return null;
  final start = order.indexOf(current as T);
  for (var step = 1; step <= order.length; step++) {
    final index = ((start == -1 ? 0 : start) + delta * step) % order.length;
    final item = order[index];
    if (item != current && test(item)) return item;
  }
  return null;
}

/// Which server is open and the last channel opened on each (§16).
@immutable
class Selection {
  const Selection({this.server, this.channels = const {}});

  final String? server;
  final Map<String, int> channels;
}

const navigationKey = 'ui.navigation';

class NavigationNotifier extends Notifier<Selection> {
  @override
  Selection build() {
    final saved = ref.watch(keyValueStoreProvider).read(navigationKey);
    if (saved == null) return const Selection();
    try {
      final json = jsonDecode(saved);
      if (json is! Map<String, Object?>) return const Selection();
      final channels = json['channels'];
      return Selection(
        server: json['server'] is String ? json['server']! as String : null,
        channels: {
          if (channels is Map<String, Object?>)
            for (final MapEntry(:key, :value) in channels.entries)
              if (value is int) key: value,
        },
      );
    } on FormatException {
      return const Selection();
    }
  }

  void openServer(String serverKey) {
    state = Selection(server: serverKey, channels: state.channels);
    _save();
  }

  void openChannel(String serverKey, int channelId) {
    state = Selection(
      server: serverKey,
      channels: {...state.channels, serverKey: channelId},
    );
    _save();
  }

  void _save() => ref
      .read(keyValueStoreProvider)
      .write(
        navigationKey,
        jsonEncode({'server': state.server, 'channels': state.channels}),
      );
}

final navigationProvider = NotifierProvider<NavigationNotifier, Selection>(
  NavigationNotifier.new,
);

/// The open server: the one chosen last if it still exists, else the
/// first in the rail.
final currentServerProvider = Provider<String?>((ref) {
  final servers = ref.watch(serverListProvider);
  final wanted = ref.watch(navigationProvider.select((s) => s.server));
  if (servers.any((server) => server.key == wanted)) return wanted;
  return servers.firstOrNull?.key;
});

/// The open channel of the open server: the last one opened there if it is
/// still visible, else the first text channel.
final currentChannelProvider = Provider<int?>((ref) {
  final server = ref.watch(currentServerProvider);
  if (server == null) return null;
  final channels = ref.watch(
    serverProvider(server).select((state) => state.data?.channels),
  );
  if (channels == null) return null;
  final wanted = ref.watch(
    navigationProvider.select((selection) => selection.channels[server]),
  );
  final chosen = channels[wanted];
  if (chosen != null && !chosen.isCategory) return chosen.id;
  return navigableChannels(channels.values).firstOrNull?.id;
});

const recentChannelsKey = 'ui.recentChannels';

/// How many recent channels are remembered.
const recentChannelsLimit = 20;

/// Channels opened most recently first, across servers: what the quick
/// switcher shows before anything is typed (§4.9).
class RecentChannelsNotifier extends Notifier<List<ChannelRef>> {
  @override
  List<ChannelRef> build() {
    final saved = ref.watch(keyValueStoreProvider).read(recentChannelsKey);
    if (saved == null) return const [];
    try {
      final json = jsonDecode(saved);
      return [
        if (json is List)
          for (final entry in json)
            if (entry is Map &&
                entry['server'] is String &&
                entry['channel'] is int)
              (
                server: entry['server'] as String,
                channel: entry['channel'] as int,
              ),
      ];
    } on FormatException {
      return const [];
    }
  }

  void visit(ChannelRef channel) {
    if (state.firstOrNull == channel) return;
    state = [
      channel,
      ...state.where((visit) => visit != channel),
    ].take(recentChannelsLimit).toList();
    ref
        .read(keyValueStoreProvider)
        .write(
          recentChannelsKey,
          jsonEncode([
            for (final visit in state)
              {'server': visit.server, 'channel': visit.channel},
          ]),
        );
  }
}

final recentChannelsProvider =
    NotifierProvider<RecentChannelsNotifier, List<ChannelRef>>(
      RecentChannelsNotifier.new,
    );

const memberPanelKey = 'ui.memberPanel';

/// Whether the member list is shown (§4.3 toggle, remembered per §16).
class MemberPanelNotifier extends Notifier<bool> {
  @override
  bool build() =>
      ref.watch(keyValueStoreProvider).read(memberPanelKey) != 'false';

  void toggle() {
    state = !state;
    ref.read(keyValueStoreProvider).write(memberPanelKey, '$state');
  }
}

final memberPanelProvider = NotifierProvider<MemberPanelNotifier, bool>(
  MemberPanelNotifier.new,
);
