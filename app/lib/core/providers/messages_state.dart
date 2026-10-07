import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/events.dart';

/// The loaded part of one channel's history plus messages still being sent.
@immutable
class ChannelMessages {
  const ChannelMessages({
    this.messages = const [],
    this.pending = const [],
    this.hasOlder = true,
    this.loaded = false,
    this.loadingOlder = false,
  });

  static const initial = ChannelMessages();

  /// Confirmed messages, oldest first.
  final List<Message> messages;

  /// Optimistic sends, oldest first; always after [messages].
  final List<Message> pending;
  final bool hasOlder;

  /// Whether the newest page has arrived.
  final bool loaded;
  final bool loadingOlder;

  List<Message> get all =>
      pending.isEmpty ? messages : [...messages, ...pending];

  ChannelMessages copyWith({
    List<Message>? messages,
    List<Message>? pending,
    bool? hasOlder,
    bool? loaded,
    bool? loadingOlder,
  }) => ChannelMessages(
    messages: messages ?? this.messages,
    pending: pending ?? this.pending,
    hasOlder: hasOlder ?? this.hasOlder,
    loaded: loaded ?? this.loaded,
    loadingOlder: loadingOlder ?? this.loadingOlder,
  );
}

/// [messages] with [message] inserted by id (or replacing the same id).
List<Message> _insert(List<Message> messages, Message message) {
  var index = messages.length;
  while (index > 0 && messages[index - 1].id >= message.id) {
    index--;
  }
  final replaces = index < messages.length && messages[index].id == message.id;
  return [
    ...messages.sublist(0, index),
    message,
    ...messages.sublist(replaces ? index + 1 : index),
  ];
}

/// Adds a fetched page. Pages are older history unless nothing is loaded
/// yet; [limit] is what was asked for, so a short page means the start of
/// the channel.
ChannelMessages withPage(
  ChannelMessages state,
  List<Message> page, {
  required int limit,
}) {
  var messages = state.messages;
  for (final message in page) {
    messages = _insert(messages, message);
  }
  return state.copyWith(
    messages: messages,
    hasOlder: page.length >= limit,
    loaded: true,
    loadingOlder: false,
  );
}

ChannelMessages withPending(ChannelMessages state, Message pending) =>
    state.copyWith(
      pending: [
        for (final message in state.pending)
          if (message.nonce != pending.nonce) message,
        pending,
      ],
    );

/// The server confirmed a send (as a response or as the event, whichever
/// came first).
ChannelMessages resolvePending(ChannelMessages state, Message sent) =>
    state.copyWith(
      messages: _insert(state.messages, sent),
      pending: [
        for (final message in state.pending)
          if (sent.nonce == null || message.nonce != sent.nonce) message,
      ],
    );

ChannelMessages failPending(ChannelMessages state, String nonce) =>
    state.copyWith(
      pending: [
        for (final message in state.pending)
          message.nonce == nonce
              ? message.copyWith(sendState: SendState.failed)
              : message,
      ],
    );

ChannelMessages removePending(ChannelMessages state, String nonce) =>
    state.copyWith(
      pending: [
        for (final message in state.pending)
          if (message.nonce != nonce) message,
      ],
    );

/// Applies a message event for this channel.
ChannelMessages reduceMessages(ChannelMessages state, RepoEvent event) {
  switch (event) {
    case MessageCreated(:final message):
      return resolvePending(state, message);
    case MessageUpdated(:final message):
      if (!state.messages.any((m) => m.id == message.id)) return state;
      return state.copyWith(messages: _insert(state.messages, message));
    case MessageDeleted(:final messageId):
      if (!state.messages.any((m) => m.id == messageId)) return state;
      return state.copyWith(
        messages: [
          for (final message in state.messages)
            if (message.id != messageId) message,
        ],
      );
    default:
      return state;
  }
}
