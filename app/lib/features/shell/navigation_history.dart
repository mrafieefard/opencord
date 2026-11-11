import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

sealed class NavEntry {
  const NavEntry();
}

class ChannelEntry extends NavEntry {
  const ChannelEntry(this.server, this.channel);

  final String server;
  final int? channel;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is ChannelEntry &&
          other.server == server &&
          other.channel == channel;

  @override
  int get hashCode => Object.hash(server, channel);

  @override
  String toString() => 'ChannelEntry(server: $server, channel: $channel)';
}

class PeerEntry extends NavEntry {
  const PeerEntry(this.peer);

  final String peer;

  @override
  bool operator ==(Object other) =>
      identical(this, other) || other is PeerEntry && other.peer == peer;

  @override
  int get hashCode => peer.hashCode;

  @override
  String toString() => 'PeerEntry(peer: $peer)';
}

class FriendsEntry extends NavEntry {
  const FriendsEntry();

  @override
  bool operator ==(Object other) => other is FriendsEntry;

  @override
  int get hashCode => (FriendsEntry).hashCode;

  @override
  String toString() => 'FriendsEntry()';
}

class RequestsEntry extends NavEntry {
  const RequestsEntry();

  @override
  bool operator ==(Object other) => other is RequestsEntry;

  @override
  int get hashCode => (RequestsEntry).hashCode;

  @override
  String toString() => 'RequestsEntry()';
}

class HistoryState {
  const HistoryState({
    required this.entries,
    required this.index,
    this.isValid,
  });

  final List<NavEntry> entries;
  final int index;
  final bool Function(NavEntry)? isValid;

  bool _checkValid(NavEntry entry) => isValid?.call(entry) ?? true;

  NavEntry? get backTarget {
    for (int i = index - 1; i >= 0; i--) {
      if (_checkValid(entries[i])) return entries[i];
    }
    return null;
  }

  bool get canBack => backTarget != null;

  NavEntry? get forwardTarget {
    for (int i = index + 1; i < entries.length; i++) {
      if (_checkValid(entries[i])) return entries[i];
    }
    return null;
  }

  bool get canForward => forwardTarget != null;

  @override
  String toString() =>
      'HistoryState(entries: $entries, index: $index, canBack: $canBack, canForward: $canForward, backTarget: $backTarget, forwardTarget: $forwardTarget)';
}

class NavigationHistory extends Notifier<HistoryState> {
  bool _stepping = false;

  @override
  HistoryState build() {
    ref.listen(navigationProvider, _onNavigationChanged);
    ref.listen<int?>(currentChannelProvider, _onChannelChanged);
    ref.listen(friendsTabProvider, _onFriendsTabChanged);
    ref.listen(serverListProvider, (_, _) => _recomputeTargets());

    final selection = ref.read(navigationProvider);
    NavEntry? initialEntry;

    if (selection.home) {
      if (selection.peer != null) {
        initialEntry = PeerEntry(selection.peer!);
      } else {
        final tab = ref.read(friendsTabProvider);
        initialEntry = tab == FriendsTab.requests
            ? const RequestsEntry()
            : const FriendsEntry();
      }
    } else if (selection.server != null) {
      final channel =
          selection.channels[selection.server!] ??
          ref.read(currentChannelProvider);
      initialEntry = ChannelEntry(selection.server!, channel);
    }

    if (initialEntry != null) {
      return _buildState(entries: [initialEntry], index: 0);
    }
    return const HistoryState(entries: [], index: -1);
  }

  void _onNavigationChanged(Selection? prev, Selection next) {
    if (_stepping) return;
    if (next.home) {
      if (next.peer != null) {
        _pushOrUpdate(PeerEntry(next.peer!));
      } else {
        final tab = ref.read(friendsTabProvider);
        if (tab == FriendsTab.requests) {
          _pushOrUpdate(const RequestsEntry());
        } else {
          _pushOrUpdate(const FriendsEntry());
        }
      }
    } else if (next.server != null) {
      final channel = next.channels[next.server!];
      if (channel != null) {
        _pushOrUpdate(ChannelEntry(next.server!, channel));
      } else {
        final serverState = ref.read(serverProvider(next.server!));
        final resolved = ref.read(currentChannelProvider);
        if (serverState.data?.channels.containsKey(resolved) ?? false) {
          _pushOrUpdate(ChannelEntry(next.server!, resolved));
        } else {
          _pushOrUpdate(ChannelEntry(next.server!, null));
        }
      }
    }
  }

  void _onChannelChanged(int? prev, int? next) {
    if (_stepping) return;
    if (state.entries.isNotEmpty &&
        state.index >= 0 &&
        state.index < state.entries.length) {
      final current = state.entries[state.index];
      final currentServer = ref.read(currentServerProvider);
      if (current is ChannelEntry &&
          currentServer != null &&
          current.server == currentServer &&
          current.channel == null &&
          next != null) {
        final newEntries = List<NavEntry>.from(state.entries);
        newEntries[state.index] = ChannelEntry(currentServer, next);
        state = _buildState(entries: newEntries, index: state.index);
      }
    }
  }

  void _onFriendsTabChanged(FriendsTab? prev, FriendsTab next) {
    if (_stepping) return;
    final home = ref.read(homeShownProvider);
    final peer = ref.read(currentPeerProvider);
    if (home && peer == null) {
      if (next == FriendsTab.requests) {
        _pushOrUpdate(const RequestsEntry());
      } else {
        _pushOrUpdate(const FriendsEntry());
      }
    }
  }

  void _recomputeTargets() {
    state = _buildState(entries: state.entries, index: state.index);
  }

  void _pushOrUpdate(NavEntry entry) {
    if (state.entries.isNotEmpty &&
        state.index >= 0 &&
        state.index < state.entries.length) {
      final current = state.entries[state.index];

      // A server's data arriving and picking its first channel replaces the entry
      if (current is ChannelEntry &&
          entry is ChannelEntry &&
          current.server == entry.server &&
          current.channel == null &&
          entry.channel != null) {
        final newEntries = List<NavEntry>.from(state.entries);
        newEntries[state.index] = entry;
        state = _buildState(entries: newEntries, index: state.index);
        return;
      }

      // Reopening the same place adds nothing
      if (current == entry) {
        return;
      }
    }

    final kept = (state.entries.isNotEmpty && state.index >= 0)
        ? state.entries.sublist(0, state.index + 1)
        : <NavEntry>[];

    final updated = [...kept, entry];
    if (updated.length > 100) {
      final trimmed = updated.sublist(updated.length - 100);
      state = _buildState(entries: trimmed, index: trimmed.length - 1);
    } else {
      state = _buildState(entries: updated, index: updated.length - 1);
    }
  }

  bool _isEntryValid(NavEntry entry) {
    switch (entry) {
      case FriendsEntry():
      case RequestsEntry():
        return true;
      case PeerEntry(:final peer):
        final contacts = ref.read(privateContactsProvider);
        final contact = contacts.byPeer[peer];
        return contact == null || !(contact.blocked || contact.hidden);
      case ChannelEntry(:final server, :final channel):
        final servers = ref.read(serverListProvider);
        if (!servers.any((s) => s.key == server)) return false;
        if (channel == null) return false;
        final serverState = ref.read(serverProvider(server));
        final channels = serverState.data?.channels;
        if (channels != null && !channels.containsKey(channel)) return false;
        final ch = channels?[channel];
        if (ch != null && cannotOpenReason(ch) != null) return false;
        return true;
    }
  }

  HistoryState _buildState({
    required List<NavEntry> entries,
    required int index,
  }) {
    return HistoryState(entries: entries, index: index, isValid: _isEntryValid);
  }

  void _applyEntry(NavEntry entry) {
    _stepping = true;
    try {
      switch (entry) {
        case ChannelEntry(:final server, :final channel):
          if (channel != null) {
            ref.read(navigationProvider.notifier).openChannel(server, channel);
          } else {
            ref.read(navigationProvider.notifier).openServer(server);
          }
        case PeerEntry(:final peer):
          ref.read(navigationProvider.notifier).openConversation(peer);
        case FriendsEntry():
          ref.read(navigationProvider.notifier).openFriends();
          final tab = ref.read(friendsTabProvider);
          if (tab == FriendsTab.requests) {
            ref.read(friendsTabProvider.notifier).select(FriendsTab.online);
          }
        case RequestsEntry():
          ref.read(navigationProvider.notifier).openFriends();
          ref.read(friendsTabProvider.notifier).select(FriendsTab.requests);
      }
    } finally {
      _stepping = false;
    }
  }

  void back() {
    for (int i = state.index - 1; i >= 0; i--) {
      final entry = state.entries[i];
      if (_isEntryValid(entry)) {
        _applyEntry(entry);
        state = _buildState(entries: state.entries, index: i);
        return;
      }
    }
  }

  void forward() {
    for (int i = state.index + 1; i < state.entries.length; i++) {
      final entry = state.entries[i];
      if (_isEntryValid(entry)) {
        _applyEntry(entry);
        state = _buildState(entries: state.entries, index: i);
        return;
      }
    }
  }
}

final navigationHistoryProvider =
    NotifierProvider<NavigationHistory, HistoryState>(NavigationHistory.new);

String navTargetTooltip(
  WidgetRef ref, {
  required bool isBack,
  required NavEntry? target,
}) {
  if (target == null) {
    return isBack ? 'Back' : 'Forward';
  }
  final prefix = isBack ? 'Back to ' : 'Forward to ';
  switch (target) {
    case FriendsEntry():
      return '${prefix}Friends';
    case RequestsEntry():
      return '${prefix}Message requests';
    case PeerEntry(:final peer):
      final name = ref.watch(contactNameProvider(peer));
      return '$prefix$name';
    case ChannelEntry(:final server, :final channel):
      final serverState = ref.watch(serverProvider(server));
      final serverName =
          serverState.data?.info.name ??
          ref
              .watch(serverListProvider)
              .where((summary) => summary.key == server)
              .firstOrNull
              ?.name ??
          server;
      final ch = channel != null ? serverState.data?.channels[channel] : null;
      if (ch != null) {
        final label = ch.kind.isTextLike ? '#${ch.name}' : ch.name;
        return '$prefix$label · $serverName';
      }
      return '$prefix$serverName';
  }
}

class HistoryArrows extends ConsumerWidget {
  const HistoryArrows({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final history = ref.watch(navigationHistoryProvider);
    final colors = context.oc;
    // A target can go while nothing moves: a chat closed, a channel deleted.
    ref.watch(privateContactsProvider);
    for (final server in {
      for (final entry in history.entries)
        if (entry is ChannelEntry) entry.server,
    }) {
      ref.watch(serverProvider(server).select((s) => s.data?.channels.length));
    }

    final backTip = navTargetTooltip(
      ref,
      isBack: true,
      target: history.backTarget,
    );
    final forwardTip = navTargetTooltip(
      ref,
      isBack: false,
      target: history.forwardTarget,
    );

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        OcIconButton(
          icon: OcIcons.arrowBack,
          tooltip: backTip,
          size: OcIconButtonSize.compact,
          color: colors.textSecondary,
          disabledColor: colors.iconDisabled,
          onPressed: history.canBack
              ? () => ref.read(navigationHistoryProvider.notifier).back()
              : null,
        ),
        const SizedBox(width: 2),
        OcIconButton(
          icon: OcIcons.arrowForward,
          tooltip: forwardTip,
          size: OcIconButtonSize.compact,
          color: colors.textSecondary,
          disabledColor: colors.iconDisabled,
          onPressed: history.canForward
              ? () => ref.read(navigationHistoryProvider.notifier).forward()
              : null,
        ),
      ],
    );
  }
}
