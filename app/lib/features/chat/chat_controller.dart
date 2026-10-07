import 'dart:ui' show VoidCallback;

/// What the message list offers the rest of the chat area.
abstract interface class MessageListHandle {
  /// Scrolls to a message, loading older history if it is not loaded yet,
  /// and highlights it briefly.
  Future<void> jumpToMessage(int messageId);

  Future<void> scrollToEnd();

  /// One screen up (-1) or down (1), for Page Up and Page Down (§7).
  void scrollPage(int direction);
}

/// Connects the pieces of one open channel: the pinned bar and reply
/// quotes jump to messages, the composer brings the list back to the
/// newest message after sending.
class ChatController implements MessageListHandle {
  MessageListHandle? _list;
  VoidCallback? _focusComposer;

  void attachComposer(VoidCallback focus) => _focusComposer = focus;

  void detachComposer(VoidCallback focus) {
    if (identical(_focusComposer, focus)) _focusComposer = null;
  }

  /// After choosing Reply or Edit, typing goes straight to the composer.
  void focusComposer() => _focusComposer?.call();

  void attach(MessageListHandle list) => _list = list;

  void detach(MessageListHandle list) {
    if (identical(_list, list)) _list = null;
  }

  @override
  Future<void> jumpToMessage(int messageId) async =>
      _list?.jumpToMessage(messageId);

  @override
  Future<void> scrollToEnd() async => _list?.scrollToEnd();

  @override
  void scrollPage(int direction) => _list?.scrollPage(direction);
}
