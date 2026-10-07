import 'dart:async';

import 'package:opencord/features/desktop/notifications.dart';

/// Records the notifications shown, and clicks them on the test's behalf.
class FakeNotifications implements NotificationService {
  final shown = <DesktopNotification>[];
  final _opened = StreamController<String>.broadcast();

  void click(DesktopNotification notification) =>
      _opened.add(notification.link);

  @override
  Future<void> show(DesktopNotification notification) async =>
      shown.add(notification);

  @override
  Stream<String> get opened => _opened.stream;

  @override
  Future<void> close() => _opened.close();
}
