import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/events.dart';

/// Pinned messages, newest first: the order the pinned bar steps through
/// them (§4.4).
List<Message> sortPins(Iterable<Message> pins) =>
    pins.toList()..sort((a, b) => b.id.compareTo(a.id));

/// Applies a message event to one channel's pins.
List<Message> reducePins(
  List<Message> pins,
  RepoEvent event, {
  required int channelId,
}) {
  switch (event) {
    case MessageUpdated(:final message) when message.channelId == channelId:
      final others = pins.where((pin) => pin.id != message.id);
      if (message.pinned) return sortPins([...others, message]);
      return others.length == pins.length ? pins : others.toList();
    case MessageDeleted(channelId: final deletedIn, :final messageId)
        when deletedIn == channelId:
      if (!pins.any((pin) => pin.id == messageId)) return pins;
      return [
        for (final pin in pins)
          if (pin.id != messageId) pin,
      ];
    default:
      return pins;
  }
}
