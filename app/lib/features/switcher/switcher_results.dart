import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/user.dart';

/// One result of the quick switcher (§4.9).
@immutable
sealed class SwitcherItem {
  const SwitcherItem();

  String get name;

  /// Channels, then servers, then members, when they rank the same.
  int get kindOrder;
}

final class SwitcherChannel extends SwitcherItem {
  const SwitcherChannel({
    required this.serverKey,
    required this.serverName,
    required this.channel,
  });

  final String serverKey;
  final String serverName;
  final Channel channel;

  @override
  String get name => channel.name;

  @override
  int get kindOrder => 0;
}

final class SwitcherServer extends SwitcherItem {
  const SwitcherServer({required this.serverKey, required this.name});

  final String serverKey;

  @override
  final String name;

  @override
  int get kindOrder => 1;
}

final class SwitcherMember extends SwitcherItem {
  const SwitcherMember({required this.serverKey, required this.member});

  final String serverKey;
  final Member member;

  @override
  String get name => member.displayName;

  @override
  int get kindOrder => 2;
}

/// The most results shown at once.
const switcherLimit = 50;

final _words = RegExp(r'[\s\-_]+');

/// 0 for a prefix match, 1 for a word that starts with [query], 2 when it
/// is anywhere in the name; null otherwise.
int? _score(String name, String query) {
  final lower = name.toLowerCase();
  if (lower.startsWith(query)) return 0;
  if (lower.split(_words).any((word) => word.startsWith(query))) return 1;
  if (lower.contains(query)) return 2;
  return null;
}

/// Ranks [items] for [query] (§4.9): prefix matches before word matches
/// before names that merely contain it. `#`, `@` and `*` in front narrow
/// to channels, members and servers. With no query, [recent] channels come
/// first.
List<SwitcherItem> rankSwitcher(
  List<SwitcherItem> items,
  String query, {
  List<({String server, int channel})> recent = const [],
}) {
  var needle = query.trim().toLowerCase();
  bool Function(SwitcherItem item) kind = (_) => true;
  if (needle.isNotEmpty) {
    switch (needle[0]) {
      case '#':
        kind = (item) => item is SwitcherChannel;
        needle = needle.substring(1);
      case '@':
        kind = (item) => item is SwitcherMember;
        needle = needle.substring(1);
      case '*':
        kind = (item) => item is SwitcherServer;
        needle = needle.substring(1);
    }
  }
  int recency(SwitcherItem item) {
    if (item is! SwitcherChannel) return recent.length;
    final index = recent.indexWhere(
      (visit) =>
          visit.server == item.serverKey && visit.channel == item.channel.id,
    );
    return index == -1 ? recent.length : index;
  }

  final scored = <(int, SwitcherItem)>[
    for (final item in items)
      if (kind(item))
        if (needle.isEmpty ? 0 : _score(item.name, needle) case final score?)
          (score, item),
  ];
  scored.sort((a, b) {
    if (a.$1 != b.$1) return a.$1 - b.$1;
    if (needle.isEmpty) {
      final byRecency = recency(a.$2) - recency(b.$2);
      if (byRecency != 0) return byRecency;
    }
    final byKind = a.$2.kindOrder - b.$2.kindOrder;
    if (byKind != 0) return byKind;
    return a.$2.name.toLowerCase().compareTo(b.$2.name.toLowerCase());
  });
  return [for (final (_, item) in scored.take(switcherLimit)) item];
}
