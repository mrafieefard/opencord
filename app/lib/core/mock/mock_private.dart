import 'dart:async';

import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/repository/repository.dart';

/// In-memory private messaging state for MockRepository.
class MockPrivateWorld {
  MockPrivateWorld({
    required this._now,
    required this._emit,
    required this._simulateLife,
  }) {
    _initData();
  }

  final DateTime _now;
  final void Function(RepoEvent) _emit;
  final bool _simulateLife;

  final Map<String, PrivateContact> contacts = {};
  final Map<String, List<PrivateMessage>> messages = {};
  var settings = const PrivacySettings(
    allowFriendRequests: true,
    allowMessageRequests: true,
  );
  var _idSequence = 1000;

  String _nextId() => (++_idSequence).toRadixString(16).padLeft(16, '0');

  void _initData() {
    final kaiKey = fakeFingerprint('key-1001');
    final miraKey = fakeFingerprint('key-1002');
    final jonasKey = fakeFingerprint('key-1003');
    final lenaKey = fakeFingerprint('key-1004');
    final priyaKey = fakeFingerprint('key-1005');
    final tomasKey = fakeFingerprint('key-1006');
    final noahKey = fakeFingerprint('key-1008');
    final gretaKey = fakeFingerprint('key-2001');
    final oldFriendKey = fakeFingerprint('old-friend');

    // 1. Friend 1: Kai Nakamura (online, accepted conversation, divider & move card)
    contacts[kaiKey] = PrivateContact(
      peer: kaiKey,
      fingerprint: '1001-KAI0-NAKA-MURA',
      name: 'Kai Nakamura',
      friend: FriendStatus.friends,
      conversation: ConversationStatus.accepted,
      relayServerKey: 'rust-berlin.example:7710',
      lastActivity: _now.subtract(const Duration(minutes: 10)),
      unread: 0,
      preview: 'Moved the chat over!',
    );

    messages[kaiKey] = [
      PrivateMessage(
        id: _nextId(),
        peer: kaiKey,
        outgoing: false,
        kind: PrivateMessageKind.divider,
        content: 'Opencord Dev',
        serverKey: 'opencord.example:7710',
        sentAt: _now.subtract(const Duration(minutes: 50)),
        state: PrivateSendState.received,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: kaiKey,
        outgoing: false,
        kind: PrivateMessageKind.text,
        content: 'Hey Alex, are you working on private messages?',
        sentAt: _now.subtract(const Duration(minutes: 45)),
        state: PrivateSendState.received,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: kaiKey,
        outgoing: true,
        kind: PrivateMessageKind.text,
        content: 'Yeah, wrapping up Milestone E4 right now!',
        sentAt: _now.subtract(const Duration(minutes: 40)),
        state: PrivateSendState.sent,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: kaiKey,
        outgoing: false,
        kind: PrivateMessageKind.text,
        content: 'Nice! Let me know if you want to test on Rust Berlin too.',
        sentAt: _now.subtract(const Duration(minutes: 30)),
        state: PrivateSendState.received,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: kaiKey,
        outgoing: false,
        kind: PrivateMessageKind.moved,
        content: 'Rust Berlin',
        serverKey: 'rust-berlin.example:7710',
        sentAt: _now.subtract(const Duration(minutes: 20)),
        state: PrivateSendState.received,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: kaiKey,
        outgoing: true,
        kind: PrivateMessageKind.text,
        content: 'Moved the chat over!',
        sentAt: _now.subtract(const Duration(minutes: 10)),
        state: PrivateSendState.sent,
      ),
    ];

    // 2. Friend 2: Mira Okafor (online, accepted conversation, divider at top)
    contacts[miraKey] = PrivateContact(
      peer: miraKey,
      fingerprint: '1002-MIRA-OKAF-0000',
      name: 'Mira Okafor',
      friend: FriendStatus.friends,
      conversation: ConversationStatus.accepted,
      relayServerKey: 'opencord.example:7710',
      lastActivity: _now.subtract(const Duration(minutes: 15)),
      unread: 1,
      preview: 'Sounds good!',
    );

    messages[miraKey] = [
      PrivateMessage(
        id: _nextId(),
        peer: miraKey,
        outgoing: false,
        kind: PrivateMessageKind.divider,
        content: 'Opencord Dev',
        serverKey: 'opencord.example:7710',
        sentAt: _now.subtract(const Duration(minutes: 45)),
        state: PrivateSendState.received,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: miraKey,
        outgoing: false,
        kind: PrivateMessageKind.text,
        content: 'Did you see the new audio changes?',
        sentAt: _now.subtract(const Duration(minutes: 40)),
        state: PrivateSendState.received,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: miraKey,
        outgoing: true,
        kind: PrivateMessageKind.text,
        content: 'Yes, the new pipeline sounds great.',
        sentAt: _now.subtract(const Duration(minutes: 35)),
        state: PrivateSendState.sent,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: miraKey,
        outgoing: false,
        kind: PrivateMessageKind.text,
        content: 'Agreed. Noise suppression is much cleaner now.',
        sentAt: _now.subtract(const Duration(minutes: 30)),
        state: PrivateSendState.received,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: miraKey,
        outgoing: true,
        kind: PrivateMessageKind.text,
        content: 'Going to test screen share next.',
        sentAt: _now.subtract(const Duration(minutes: 20)),
        state: PrivateSendState.sent,
      ),
      PrivateMessage(
        id: _nextId(),
        peer: miraKey,
        outgoing: false,
        kind: PrivateMessageKind.text,
        content: 'Sounds good!',
        sentAt: _now.subtract(const Duration(minutes: 15)),
        state: PrivateSendState.received,
      ),
    ];

    // 3. Friend 3: Noah Kim (offline)
    contacts[noahKey] = PrivateContact(
      peer: noahKey,
      fingerprint: '1008-NOAH-KIM0-0000',
      name: 'Noah Kim',
      friend: FriendStatus.friends,
      conversation: ConversationStatus.none,
      relayServerKey: 'opencord.example:7710',
      lastActivity: null,
      unread: 0,
    );

    // 4. Incoming request: Jonas Weber (not spam)
    contacts[jonasKey] = PrivateContact(
      peer: jonasKey,
      fingerprint: '1003-JONA-SWE0-0000',
      name: 'Jonas Weber',
      friend: FriendStatus.incoming,
      requestSpam: false,
      conversation: ConversationStatus.none,
      relayServerKey: 'opencord.example:7710',
      lastActivity: _now.subtract(const Duration(hours: 2)),
      unread: 0,
    );

    // 5. Spam request: Lena Fischer
    contacts[lenaKey] = PrivateContact(
      peer: lenaKey,
      fingerprint: '1004-LENA-FISC-0000',
      name: 'Lena Fischer',
      friend: FriendStatus.incoming,
      requestSpam: true,
      conversation: ConversationStatus.none,
      relayServerKey: 'opencord.example:7710',
      lastActivity: _now.subtract(const Duration(days: 1)),
      unread: 0,
    );

    // 6. Outgoing request: Tomás Ruiz
    contacts[tomasKey] = PrivateContact(
      peer: tomasKey,
      fingerprint: '1006-TOMA-SRUI-0000',
      name: 'Tomás Ruiz',
      friend: FriendStatus.outgoing,
      conversation: ConversationStatus.none,
      relayServerKey: 'opencord.example:7710',
      lastActivity: _now.subtract(const Duration(days: 2)),
      unread: 0,
    );

    // 7. Message request 1: Priya Shah
    contacts[priyaKey] = PrivateContact(
      peer: priyaKey,
      fingerprint: '1005-PRIY-ASHA-0000',
      name: 'Priya Shah',
      friend: FriendStatus.none,
      conversation: ConversationStatus.request,
      relayServerKey: 'opencord.example:7710',
      lastActivity: _now.subtract(const Duration(hours: 3)),
      unread: 1,
      preview: 'Hi! Do you have a moment to chat about the docs?',
    );
    messages[priyaKey] = [
      PrivateMessage(
        id: _nextId(),
        peer: priyaKey,
        outgoing: false,
        kind: PrivateMessageKind.text,
        content: 'Hi! Do you have a moment to chat about the docs?',
        sentAt: _now.subtract(const Duration(hours: 3)),
        state: PrivateSendState.received,
      ),
    ];

    // 8. Message request 2: Greta Hoffmann (Rust Berlin)
    contacts[gretaKey] = PrivateContact(
      peer: gretaKey,
      fingerprint: '2001-GRET-AHOF-0000',
      name: 'Greta Hoffmann',
      friend: FriendStatus.none,
      conversation: ConversationStatus.request,
      relayServerKey: 'rust-berlin.example:7710',
      lastActivity: _now.subtract(const Duration(hours: 5)),
      unread: 1,
      preview:
          'Hello! We would love to have you speak at the next Berlin meetup.',
    );
    messages[gretaKey] = [
      PrivateMessage(
        id: _nextId(),
        peer: gretaKey,
        outgoing: false,
        kind: PrivateMessageKind.text,
        content:
            'Hello! We would love to have you speak at the next Berlin meetup.',
        sentAt: _now.subtract(const Duration(hours: 5)),
        state: PrivateSendState.received,
      ),
    ];

    // 9. Blocked person: Old Friend (no shared server)
    contacts[oldFriendKey] = PrivateContact(
      peer: oldFriendKey,
      fingerprint: 'ABCD-EFGH-1234-5678',
      name: 'Old Friend',
      friend: FriendStatus.none,
      blocked: true,
      conversation: ConversationStatus.none,
      lastActivity: null,
      unread: 0,
    );
  }

  Future<List<PrivateContact>> privateContacts() async =>
      contacts.values.toList();

  Future<List<PrivateMessage>> privateMessages(
    String peer, {
    String? before,
    int limit = 50,
  }) async {
    final list = messages[peer] ?? [];
    var filtered = list;
    if (before != null) {
      final index = list.indexWhere((m) => m.id == before);
      if (index > 0) {
        filtered = list.sublist(0, index);
      } else if (index == 0) {
        filtered = [];
      }
    }
    // Return newest first, up to limit
    final reversed = filtered.reversed.take(limit).toList();
    return reversed;
  }

  Future<PrivateMessage> sendPrivateMessage(
    String peer,
    String content, {
    required DateTime now,
  }) async {
    final msg = PrivateMessage(
      id: _nextId(),
      peer: peer,
      outgoing: true,
      kind: PrivateMessageKind.text,
      content: content,
      sentAt: now,
      state: PrivateSendState.sending,
    );

    final list = messages.putIfAbsent(peer, () => []);
    list.add(msg);

    final contact =
        (contacts[peer] ??
                PrivateContact(
                  peer: peer,
                  fingerprint: fakeFingerprint(peer).substring(0, 19),
                  friend: FriendStatus.none,
                  conversation: ConversationStatus.accepted,
                ))
            .copyWith(lastActivity: now, preview: content);
    contacts[peer] = contact;

    _emit(PrivateMessageAdded(msg));

    void markSent() {
      final sentMsg = msg.copyWith(state: PrivateSendState.sent);
      final idx = list.indexWhere((m) => m.id == msg.id);
      if (idx != -1) list[idx] = sentMsg;
      _emit(PrivateMessageChanged(sentMsg));
      _emit(PrivateContactChanged(contact));
    }

    if (_simulateLife) {
      Timer(const Duration(milliseconds: 300), markSent);
    } else {
      markSent();
    }

    return msg;
  }

  Future<void> editPrivateMessage(
    String peer,
    String messageId,
    String content, {
    required DateTime now,
  }) async {
    final list = messages[peer];
    if (list == null) return;
    final idx = list.indexWhere((m) => m.id == messageId);
    if (idx == -1) return;
    final edited = list[idx].copyWith(content: content, editedAt: now);
    list[idx] = edited;
    _emit(PrivateMessageChanged(edited));
  }

  Future<void> deletePrivateMessage(String peer, String messageId) async {
    final list = messages[peer];
    if (list == null) return;
    list.removeWhere((m) => m.id == messageId);
    _emit(PrivateMessageDeleted(peer, messageId));
  }

  Future<void> retryPrivateMessage(String messageId) async {
    for (final entry in messages.entries) {
      final idx = entry.value.indexWhere((m) => m.id == messageId);
      if (idx != -1) {
        final retried = entry.value[idx].copyWith(
          state: PrivateSendState.sending,
        );
        entry.value[idx] = retried;
        _emit(PrivateMessageChanged(retried));
        final sent = retried.copyWith(state: PrivateSendState.sent);
        entry.value[idx] = sent;
        _emit(PrivateMessageChanged(sent));
        return;
      }
    }
  }

  Future<void> sendFriendRequest(
    String serverKey,
    int userId, {
    required DateTime now,
  }) async {
    if (userId == 1013) {
      throw const RepoException(
        RepoErrorKind.friendRequestsClosed,
        "This user isn't accepting friend requests.",
      );
    }
    final peerKey = fakeFingerprint('key-$userId');
    final contact =
        (contacts[peerKey] ??
                PrivateContact(
                  peer: peerKey,
                  fingerprint: fakeFingerprint(peerKey).substring(0, 19),
                  friend: FriendStatus.outgoing,
                  conversation: ConversationStatus.none,
                  relayServerKey: serverKey,
                ))
            .copyWith(friend: FriendStatus.outgoing, relayServerKey: serverKey);
    contacts[peerKey] = contact;
    _emit(PrivateContactChanged(contact));
  }

  Future<void> answerFriendRequest(String peer, {required bool accept}) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(
      friend: accept ? FriendStatus.friends : FriendStatus.none,
    );
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }

  Future<void> cancelFriendRequest(String peer) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(friend: FriendStatus.none);
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }

  Future<void> removeFriend(String peer) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(friend: FriendStatus.none);
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }

  Future<void> acceptMessageRequest(String peer) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(conversation: ConversationStatus.accepted);
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }

  Future<void> ignoreMessageRequest(String peer) async {
    contacts.remove(peer);
    messages.remove(peer);
    _emit(PrivateContactRemoved(peer));
  }

  Future<void> blockUser(String peer) async {
    final contact =
        contacts[peer] ??
        PrivateContact(
          peer: peer,
          fingerprint: fakeFingerprint(peer).substring(0, 19),
          friend: FriendStatus.none,
          conversation: ConversationStatus.none,
        );
    final next = contact.copyWith(blocked: true, friend: FriendStatus.none);
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }

  Future<void> unblockUser(String peer) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(blocked: false);
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }

  Future<PrivacySettings> privacySettings() async => settings;

  Future<void> savePrivacySettings(PrivacySettings s) async {
    settings = s;
  }

  Future<PrivateContact> openConversation(
    String serverKey,
    int userId, {
    required DateTime now,
  }) async {
    final peerKey = fakeFingerprint('key-$userId');
    final contact =
        (contacts[peerKey] ??
                PrivateContact(
                  peer: peerKey,
                  fingerprint: fakeFingerprint(peerKey).substring(0, 19),
                  friend: FriendStatus.none,
                  conversation: ConversationStatus.accepted,
                  relayServerKey: serverKey,
                ))
            .copyWith(
              conversation: ConversationStatus.accepted,
              relayServerKey: serverKey,
            );
    contacts[peerKey] = contact;
    _emit(PrivateContactChanged(contact));
    return contact;
  }

  Future<void> moveConversation(
    String peer,
    String serverKey, {
    required String serverName,
    required DateTime now,
  }) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(relayServerKey: serverKey);
    contacts[peer] = next;

    final div = PrivateMessage(
      id: _nextId(),
      peer: peer,
      outgoing: true,
      kind: PrivateMessageKind.divider,
      content: serverName,
      serverKey: serverKey,
      sentAt: now,
      state: PrivateSendState.sent,
    );
    messages.putIfAbsent(peer, () => []).add(div);
    _emit(PrivateMessageAdded(div));
    _emit(PrivateContactChanged(next));
  }

  Future<void> moveConversationToo(
    String peer,
    String messageId, {
    required DateTime now,
  }) async {
    final list = messages[peer] ?? [];
    final card = list.firstWhere(
      (m) => m.id == messageId && m.kind == PrivateMessageKind.moved,
    );
    if (card.serverKey == null) return;
    await moveConversation(
      peer,
      card.serverKey!,
      serverName: card.content,
      now: now,
    );
  }

  Future<void> sendPrivateTyping(String peer) async {
    _emit(PrivateTypingStarted(peer));
  }

  Future<void> markPrivateRead(String peer) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(unread: 0);
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }

  Future<void> setPrivateMuted(String peer, {required bool muted}) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(muted: muted);
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }

  Future<void> hideConversation(String peer) async {
    final contact = contacts[peer];
    if (contact == null) return;
    final next = contact.copyWith(hidden: true);
    contacts[peer] = next;
    _emit(PrivateContactChanged(next));
  }
}
