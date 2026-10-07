import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// A desktop notification for a new message (§15).
@immutable
class DesktopNotification {
  const DesktopNotification({
    required this.title,
    required this.body,
    required this.link,
    this.sound = false,
  });

  final String title;
  final String body;

  /// The message's `opencord://` link, opened on a click.
  final String link;

  /// Asks the desktop to play its new-message sound.
  final bool sound;
}

/// The desktop's notifications.
abstract interface class NotificationService {
  Future<void> show(DesktopNotification notification);

  /// Links of the notifications clicked.
  Stream<String> get opened;

  Future<void> close();
}

/// Where notifications are not wired yet, and in tests.
class NoNotifications implements NotificationService {
  const NoNotifications();

  @override
  Future<void> show(DesktopNotification notification) async {}

  @override
  Stream<String> get opened => const Stream.empty();

  @override
  Future<void> close() async {}
}

/// Overridden in `main` where the platform has notifications.
final notificationServiceProvider = Provider<NotificationService>(
  (ref) => const NoNotifications(),
);
