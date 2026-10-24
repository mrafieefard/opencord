import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';

/// A server shared with a peer where both users are members (§6.3, §7.5).
@immutable
class SharedServerView {
  const SharedServerView({
    required this.serverKey,
    required this.name,
    required this.peerUserId,
  });

  final String serverKey;
  final String name;
  final int peerUserId;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is SharedServerView &&
          other.serverKey == serverKey &&
          other.name == name &&
          other.peerUserId == peerUserId;

  @override
  int get hashCode => Object.hash(serverKey, name, peerUserId);

  @override
  String toString() =>
      'SharedServerView(serverKey: $serverKey, name: $name, peerUserId: $peerUserId)';
}

/// Servers in rail order where this device has data, it is connected,
/// privateMessages is enabled, and a member has `user.publicKeyHex == peer`.
final sharedServersProvider = Provider.family<List<SharedServerView>, String>((
  ref,
  peer,
) {
  final servers = ref.watch(serverListProvider);
  final shared = <SharedServerView>[];
  for (final summary in servers) {
    final server = ref.watch(serverProvider(summary.key));
    if (server.connection.phase != ConnectionPhase.connected) continue;
    final data = server.data;
    if (data == null || !data.privateMessages) continue;
    final member = data.members.values
        .where((m) => m.user.publicKeyHex == peer)
        .firstOrNull;
    if (member != null) {
      shared.add(
        SharedServerView(
          serverKey: summary.key,
          name: data.info.name,
          peerUserId: member.id,
        ),
      );
    }
  }
  return shared;
});

/// Resolves a contact's display name according to spec §7.5:
/// 1. The relay server's member name if member there
/// 2. The first shared server in rail order
/// 3. contact.name
/// 4. Fingerprint (or peer key)
final contactNameProvider = Provider.family<String, String>((ref, peer) {
  final contacts = ref.watch(privateContactsProvider);
  final contact = contacts.byPeer[peer];

  // 1. Relay server
  if (contact?.relayServerKey != null) {
    final relayServer = ref.watch(serverProvider(contact!.relayServerKey!));
    final member = relayServer.data?.members.values
        .where((m) => m.user.publicKeyHex == peer)
        .firstOrNull;
    if (member != null && member.displayName.isNotEmpty) {
      return member.displayName;
    }
  }

  // 2. First shared server in rail order
  final shared = ref.watch(sharedServersProvider(peer));
  for (final s in shared) {
    final server = ref.watch(serverProvider(s.serverKey));
    final member = server.data?.members[s.peerUserId];
    if (member != null && member.displayName.isNotEmpty) {
      return member.displayName;
    }
  }

  // 3. contact.name
  if (contact?.name != null && contact!.name!.isNotEmpty) {
    return contact.name!;
  }

  // 4. Fingerprint
  if (contact?.fingerprint != null && contact!.fingerprint.isNotEmpty) {
    return contact.fingerprint;
  }

  return peer;
});

/// Resolves a contact's avatar reference according to spec §7.5:
/// Relay server member avatar first, then first shared server in rail order;
/// null if no shared server.
final contactAvatarProvider =
    Provider.family<({String serverKey, int userId})?, String>((ref, peer) {
      final contacts = ref.watch(privateContactsProvider);
      final contact = contacts.byPeer[peer];

      // 1. Relay server
      if (contact?.relayServerKey != null) {
        final relayServer = ref.watch(serverProvider(contact!.relayServerKey!));
        final member = relayServer.data?.members.values
            .where((m) => m.user.publicKeyHex == peer)
            .firstOrNull;
        if (member != null) {
          return (serverKey: contact.relayServerKey!, userId: member.id);
        }
      }

      // 2. First shared server in rail order
      final shared = ref.watch(sharedServersProvider(peer));
      if (shared.isNotEmpty) {
        return (
          serverKey: shared.first.serverKey,
          userId: shared.first.peerUserId,
        );
      }

      return null;
    });

/// Resolves a contact's presence according to spec §7.5:
/// From relay server if member there and connected, else from the first
/// shared, connected server that shows them not offline; else offline.
final contactPresenceProvider = Provider.family<Presence, String>((ref, peer) {
  final contacts = ref.watch(privateContactsProvider);
  final contact = contacts.byPeer[peer];

  // 1. Relay server if connected
  if (contact?.relayServerKey != null) {
    final relayServer = ref.watch(serverProvider(contact!.relayServerKey!));
    if (relayServer.connection.phase == ConnectionPhase.connected &&
        relayServer.data != null) {
      final member = relayServer.data!.members.values
          .where((m) => m.user.publicKeyHex == peer)
          .firstOrNull;
      if (member != null) {
        return ref
            .watch(presenceProvider(contact.relayServerKey!))
            .of(member.id);
      }
    }
  }

  // 2. First shared connected server not offline
  final shared = ref.watch(sharedServersProvider(peer));
  for (final s in shared) {
    final p = ref.watch(presenceProvider(s.serverKey)).of(s.peerUserId);
    if (p != Presence.offline) return p;
  }

  return Presence.offline;
});

/// Unread private messages and incoming friend requests (§6, §17).
final privateUnreadProvider = Provider<int>((ref) {
  final contactsState = ref.watch(privateContactsProvider);
  var count = 0;
  for (final contact in contactsState.byPeer.values) {
    if (contact.blocked) continue;
    if (contact.conversation == ConversationStatus.accepted && !contact.muted) {
      count += contact.unread;
    }
    if (contact.friend == FriendStatus.incoming && !contact.requestSpam) {
      count += 1;
    }
  }
  return count;
});

@immutable
class PrivateContactsState {
  const PrivateContactsState({
    this.byPeer = const {},
    this.loaded = false,
    this.error,
  });

  final Map<String, PrivateContact> byPeer;
  final bool loaded;
  final RepoException? error;

  List<PrivateContact> get all => byPeer.values.toList();
}

class PrivateContactsNotifier extends Notifier<PrivateContactsState> {
  @override
  PrivateContactsState build() {
    _load();
    return const PrivateContactsState();
  }

  Future<void> _load() async {
    try {
      final list = await ref.read(repositoryProvider).privateContacts();
      if (!ref.mounted) return;
      state = PrivateContactsState(
        byPeer: {for (final c in list) c.peer: c},
        loaded: true,
      );
    } on RepoException catch (e) {
      if (!ref.mounted) return;
      state = PrivateContactsState(
        byPeer: state.byPeer,
        loaded: true,
        error: e,
      );
    }
  }

  void onContactChanged(PrivateContact contact) {
    final next = {...state.byPeer, contact.peer: contact};
    state = PrivateContactsState(
      byPeer: next,
      loaded: state.loaded,
      error: state.error,
    );
  }

  void onContactRemoved(String peer) {
    if (!state.byPeer.containsKey(peer)) return;
    final next = {...state.byPeer}..remove(peer);
    state = PrivateContactsState(
      byPeer: next,
      loaded: state.loaded,
      error: state.error,
    );
  }

  void onStateChanged() {
    _load();
  }
}

final privateContactsProvider =
    NotifierProvider<PrivateContactsNotifier, PrivateContactsState>(
      PrivateContactsNotifier.new,
    );

@immutable
class PrivateConversationState {
  const PrivateConversationState({
    this.messages = const [],
    this.loading = false,
    this.hasMore = false,
    this.error,
  });

  final List<PrivateMessage> messages;
  final bool loading;
  final bool hasMore;
  final RepoException? error;
}

int _compareMessages(PrivateMessage a, PrivateMessage b) {
  final byTime = a.sentAt.compareTo(b.sentAt);
  if (byTime != 0) return byTime;
  return a.id.compareTo(b.id);
}

class PrivateConversationNotifier extends Notifier<PrivateConversationState> {
  PrivateConversationNotifier(this.peer);

  final String peer;

  @override
  PrivateConversationState build() {
    _loadInitial();
    return const PrivateConversationState(loading: true);
  }

  Future<void> _loadInitial() async {
    try {
      final batch = await ref
          .read(repositoryProvider)
          .privateMessages(peer, limit: 50);
      if (!ref.mounted) return;
      final sorted = List<PrivateMessage>.from(batch)..sort(_compareMessages);
      state = PrivateConversationState(
        messages: sorted,
        loading: false,
        hasMore: batch.length >= 50,
      );
    } on RepoException catch (e) {
      if (!ref.mounted) return;
      state = PrivateConversationState(
        messages: state.messages,
        loading: false,
        error: e,
      );
    }
  }

  Future<void> loadOlder() async {
    if (state.loading || !state.hasMore || state.messages.isEmpty) return;
    final oldestId = state.messages.first.id;
    state = PrivateConversationState(
      messages: state.messages,
      loading: true,
      hasMore: state.hasMore,
    );
    try {
      final older = await ref
          .read(repositoryProvider)
          .privateMessages(peer, before: oldestId, limit: 50);
      if (!ref.mounted) return;
      final merged = [
        ...List<PrivateMessage>.from(older)..sort(_compareMessages),
        ...state.messages,
      ];
      state = PrivateConversationState(
        messages: merged,
        loading: false,
        hasMore: older.length >= 50,
      );
    } on RepoException catch (e) {
      if (!ref.mounted) return;
      state = PrivateConversationState(
        messages: state.messages,
        loading: false,
        hasMore: state.hasMore,
        error: e,
      );
    }
  }

  void onMessageAdded(PrivateMessage message) {
    if (message.peer != peer) return;
    final existing = state.messages.indexWhere((m) => m.id == message.id);
    if (existing != -1) {
      final updated = List<PrivateMessage>.from(state.messages)
        ..[existing] = message;
      state = PrivateConversationState(
        messages: updated,
        loading: state.loading,
        hasMore: state.hasMore,
        error: state.error,
      );
      return;
    }
    final merged = [...state.messages, message]..sort(_compareMessages);
    state = PrivateConversationState(
      messages: merged,
      loading: state.loading,
      hasMore: state.hasMore,
      error: state.error,
    );
  }

  void onMessageChanged(PrivateMessage message) {
    if (message.peer != peer) return;
    final idx = state.messages.indexWhere((m) => m.id == message.id);
    if (idx == -1) return;
    final updated = List<PrivateMessage>.from(state.messages)..[idx] = message;
    state = PrivateConversationState(
      messages: updated,
      loading: state.loading,
      hasMore: state.hasMore,
      error: state.error,
    );
  }

  void onMessageDeleted(String messageId) {
    final idx = state.messages.indexWhere((m) => m.id == messageId);
    if (idx == -1) return;
    final updated = List<PrivateMessage>.from(state.messages)..removeAt(idx);
    state = PrivateConversationState(
      messages: updated,
      loading: state.loading,
      hasMore: state.hasMore,
      error: state.error,
    );
  }
}

final privateConversationProvider =
    NotifierProvider.family<
      PrivateConversationNotifier,
      PrivateConversationState,
      String
    >(PrivateConversationNotifier.new);

const privateTypingTimeout = Duration(seconds: 8);

class PrivateTypingNotifier extends Notifier<Map<String, DateTime>> {
  Timer? _prune;

  @override
  Map<String, DateTime> build() {
    ref.onDispose(() => _prune?.cancel());
    return const {};
  }

  void onTyping(String peer) {
    final now = ref.read(clockProvider)();
    final until = now.add(privateTypingTimeout);
    final next = {...state, peer: until};
    state = _pruned(next, now);
    _schedulePrune(now);
  }

  void onMessage(String peer) {
    if (!state.containsKey(peer)) return;
    final next = {...state}..remove(peer);
    state = next;
  }

  Map<String, DateTime> _pruned(Map<String, DateTime> map, DateTime now) {
    final live = <String, DateTime>{};
    for (final MapEntry(:key, :value) in map.entries) {
      if (value.isAfter(now)) live[key] = value;
    }
    return live;
  }

  void _schedulePrune(DateTime now) {
    _prune?.cancel();
    if (state.isEmpty) return;
    DateTime? earliest;
    for (final expiry in state.values) {
      if (earliest == null || expiry.isBefore(earliest)) earliest = expiry;
    }
    if (earliest == null) return;
    final delay = earliest.difference(now) + const Duration(milliseconds: 1);
    _prune = Timer(delay.isNegative ? Duration.zero : delay, () {
      if (!ref.mounted) return;
      final later = ref.read(clockProvider)();
      state = _pruned(state, later);
      _schedulePrune(later);
    });
  }
}

final privateTypingProvider =
    NotifierProvider<PrivateTypingNotifier, Map<String, DateTime>>(
      PrivateTypingNotifier.new,
    );
