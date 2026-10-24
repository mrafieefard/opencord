import 'package:flutter/foundation.dart';

const Object _sentinel = Object();

/// Friendship status between the local user and a contact (§7.2, §7.3).
enum FriendStatus { none, outgoing, incoming, friends }

/// Conversation status between the local user and a contact (§6.3, §7.1).
enum ConversationStatus { none, request, accepted }

/// The kind of entry in a private conversation (§6.3).
enum PrivateMessageKind { text, divider, moved, undecryptable }

/// The state of an outbound or inbound private message.
enum PrivateSendState { received, sending, sent, failed }

/// A person known to private messages (a friend, a conversation, a request,
/// or blocked).
@immutable
class PrivateContact {
  const PrivateContact({
    required this.peer,
    required this.fingerprint,
    this.name,
    required this.friend,
    this.requestSpam = false,
    this.blocked = false,
    required this.conversation,
    this.relayServerKey,
    this.lastActivity,
    this.unread = 0,
    this.muted = false,
    this.hidden = false,
    this.preview = '',
  });

  /// Lowercase hex identity public key.
  final String peer;

  /// Formatted key fingerprint (e.g. `ABCD-EFGH-IJKL-MNOP`).
  final String fingerprint;

  /// The display name last seen.
  final String? name;

  final FriendStatus friend;
  final bool requestSpam;
  final bool blocked;
  final ConversationStatus conversation;

  /// The server key of the relay server used for sending, if known to this
  /// device.
  final String? relayServerKey;

  /// The timestamp of the newest message or activity.
  final DateTime? lastActivity;

  /// Count of unread incoming text messages.
  final int unread;

  final bool muted;
  final bool hidden;

  /// Preview of the newest message (at most 100 characters).
  final String preview;

  PrivateContact copyWith({
    String? peer,
    String? fingerprint,
    Object? name = _sentinel,
    FriendStatus? friend,
    bool? requestSpam,
    bool? blocked,
    ConversationStatus? conversation,
    Object? relayServerKey = _sentinel,
    Object? lastActivity = _sentinel,
    int? unread,
    bool? muted,
    bool? hidden,
    String? preview,
  }) => PrivateContact(
    peer: peer ?? this.peer,
    fingerprint: fingerprint ?? this.fingerprint,
    name: identical(name, _sentinel) ? this.name : name as String?,
    friend: friend ?? this.friend,
    requestSpam: requestSpam ?? this.requestSpam,
    blocked: blocked ?? this.blocked,
    conversation: conversation ?? this.conversation,
    relayServerKey: identical(relayServerKey, _sentinel)
        ? this.relayServerKey
        : relayServerKey as String?,
    lastActivity: identical(lastActivity, _sentinel)
        ? this.lastActivity
        : lastActivity as DateTime?,
    unread: unread ?? this.unread,
    muted: muted ?? this.muted,
    hidden: hidden ?? this.hidden,
    preview: preview ?? this.preview,
  );

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is PrivateContact &&
          other.peer == peer &&
          other.fingerprint == fingerprint &&
          other.name == name &&
          other.friend == friend &&
          other.requestSpam == requestSpam &&
          other.blocked == blocked &&
          other.conversation == conversation &&
          other.relayServerKey == relayServerKey &&
          other.lastActivity == lastActivity &&
          other.unread == unread &&
          other.muted == muted &&
          other.hidden == hidden &&
          other.preview == preview;

  @override
  int get hashCode => Object.hash(
    peer,
    fingerprint,
    name,
    friend,
    requestSpam,
    blocked,
    conversation,
    relayServerKey,
    lastActivity,
    unread,
    muted,
    hidden,
    preview,
  );

  @override
  String toString() =>
      'PrivateContact(peer: $peer, name: $name, friend: $friend, conversation: $conversation)';
}

/// One message, divider, or move card in a private conversation.
@immutable
class PrivateMessage {
  const PrivateMessage({
    required this.id,
    required this.peer,
    required this.outgoing,
    required this.kind,
    required this.content,
    this.serverKey,
    required this.sentAt,
    this.editedAt,
    required this.state,
  });

  /// Hex message identifier.
  final String id;

  /// The other participant's public key hex.
  final String peer;

  /// Whether sent from this device/user.
  final bool outgoing;

  final PrivateMessageKind kind;

  /// The text content; or the server name for dividers/move cards.
  final String content;

  /// The server key for dividers or move cards, if known.
  final String? serverKey;

  final DateTime sentAt;
  final DateTime? editedAt;
  final PrivateSendState state;

  PrivateMessage copyWith({
    String? id,
    String? peer,
    bool? outgoing,
    PrivateMessageKind? kind,
    String? content,
    Object? serverKey = _sentinel,
    DateTime? sentAt,
    Object? editedAt = _sentinel,
    PrivateSendState? state,
  }) => PrivateMessage(
    id: id ?? this.id,
    peer: peer ?? this.peer,
    outgoing: outgoing ?? this.outgoing,
    kind: kind ?? this.kind,
    content: content ?? this.content,
    serverKey: identical(serverKey, _sentinel)
        ? this.serverKey
        : serverKey as String?,
    sentAt: sentAt ?? this.sentAt,
    editedAt: identical(editedAt, _sentinel)
        ? this.editedAt
        : editedAt as DateTime?,
    state: state ?? this.state,
  );

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is PrivateMessage &&
          other.id == id &&
          other.peer == peer &&
          other.outgoing == outgoing &&
          other.kind == kind &&
          other.content == content &&
          other.serverKey == serverKey &&
          other.sentAt == sentAt &&
          other.editedAt == editedAt &&
          other.state == state;

  @override
  int get hashCode => Object.hash(
    id,
    peer,
    outgoing,
    kind,
    content,
    serverKey,
    sentAt,
    editedAt,
    state,
  );

  @override
  String toString() =>
      'PrivateMessage(id: $id, peer: $peer, outgoing: $outgoing, kind: $kind, state: $state)';
}

/// The user's privacy settings controlling inbound friend and message requests.
@immutable
class PrivacySettings {
  const PrivacySettings({
    this.allowFriendRequests = true,
    this.allowMessageRequests = true,
  });

  final bool allowFriendRequests;
  final bool allowMessageRequests;

  PrivacySettings copyWith({
    bool? allowFriendRequests,
    bool? allowMessageRequests,
  }) => PrivacySettings(
    allowFriendRequests: allowFriendRequests ?? this.allowFriendRequests,
    allowMessageRequests: allowMessageRequests ?? this.allowMessageRequests,
  );

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is PrivacySettings &&
          other.allowFriendRequests == allowFriendRequests &&
          other.allowMessageRequests == allowMessageRequests;

  @override
  int get hashCode => Object.hash(allowFriendRequests, allowMessageRequests);

  @override
  String toString() =>
      'PrivacySettings(allowFriendRequests: $allowFriendRequests, allowMessageRequests: $allowMessageRequests)';
}
