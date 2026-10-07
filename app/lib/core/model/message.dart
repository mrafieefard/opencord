import 'package:flutter/foundation.dart';

/// A small notice in the history instead of a bubble (§4.5).
enum SystemEvent { memberJoined, channelCreated, messagePinned }

/// Where an outgoing message is (§6): pending shows a clock, sent a double
/// check, failed an outlined error with Retry.
enum SendState { sent, pending, failed }

@immutable
class Reaction {
  const Reaction({
    required this.emoji,
    required this.userIds,
    required this.me,
  });

  final String emoji;

  /// Who reacted, for the hover tooltip.
  final List<int> userIds;

  /// Whether the current user is among them.
  final bool me;

  int get count => userIds.length;
}

@immutable
class Message {
  const Message({
    required this.id,
    required this.channelId,
    required this.authorId,
    required this.content,
    required this.createdAt,
    this.editedAt,
    this.nonce,
    this.replyToId,
    this.reactions = const [],
    this.pinned = false,
    this.systemEvent,
    this.sendState = SendState.sent,
  });

  /// A snowflake: larger ids are newer. Pending messages use a placeholder.
  final int id;
  final int channelId;
  final int authorId;
  final String content;
  final DateTime createdAt;
  final DateTime? editedAt;

  /// Set on messages this client sent, to match the server's copy.
  final String? nonce;
  final int? replyToId;
  final List<Reaction> reactions;
  final bool pinned;

  /// Set on system notices; [authorId] is then who caused it.
  final SystemEvent? systemEvent;
  final SendState sendState;

  bool get isSystem => systemEvent != null;

  bool get edited => editedAt != null;

  Message copyWith({
    int? id,
    String? content,
    DateTime? Function()? editedAt,
    List<Reaction>? reactions,
    bool? pinned,
    SendState? sendState,
    DateTime? createdAt,
  }) => Message(
    id: id ?? this.id,
    channelId: channelId,
    authorId: authorId,
    content: content ?? this.content,
    createdAt: createdAt ?? this.createdAt,
    editedAt: editedAt == null ? this.editedAt : editedAt(),
    nonce: nonce,
    replyToId: replyToId,
    reactions: reactions ?? this.reactions,
    pinned: pinned ?? this.pinned,
    systemEvent: systemEvent,
    sendState: sendState ?? this.sendState,
  );
}
