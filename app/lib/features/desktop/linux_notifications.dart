import 'dart:async';
import 'dart:developer' as developer;

import 'package:dbus/dbus.dart';

import 'package:opencord/features/desktop/notifications.dart';

const _service = 'org.freedesktop.Notifications';

/// How long a notification daemon may take to answer, including being
/// started by D-Bus activation.
const _answerWithin = Duration(seconds: 5);

/// Notifications through `org.freedesktop.Notifications` on the session
/// bus (§15), which every Linux desktop's notification daemon serves.
class LinuxNotifications implements NotificationService {
  LinuxNotifications._(this._client);

  final DBusClient _client;
  final _opened = StreamController<String>.broadcast();

  /// Links by notification id, until each is closed.
  final _links = <int, String>{};
  final _subscriptions = <StreamSubscription<Object>>[];

  /// False once no daemon answered, until one appears on the bus: a
  /// desktop without a notification daemon must not collect hung calls.
  bool get usable => _usable;
  var _usable = true;

  /// Notifications on the session bus, or null when there is none.
  static Future<LinuxNotifications?> start() async {
    final DBusClient client;
    try {
      client = DBusClient.session();
    } on Exception catch (error) {
      developer.log('No session bus for notifications: $error', name: 'notify');
      return null;
    }
    final notifications = LinuxNotifications._(client);
    notifications._subscriptions.addAll([
      DBusSignalStream(
        client,
        interface: _service,
        name: 'ActionInvoked',
        signature: DBusSignature('us'),
      ).listen(notifications._onAction),
      DBusSignalStream(
        client,
        interface: _service,
        name: 'NotificationClosed',
        signature: DBusSignature('uu'),
      ).listen(
        (signal) => notifications._links.remove(signal.values[0].asUint32()),
      ),
      client.nameOwnerChanged
          .where((event) => event.name == _service && event.newOwner != null)
          .listen((_) => notifications._usable = true),
    ]);
    return notifications;
  }

  void _onAction(DBusSignal signal) {
    final link = _links[signal.values[0].asUint32()];
    if (link != null && signal.values[1].asString() == 'default') {
      _opened.add(link);
    }
  }

  @override
  Stream<String> get opened => _opened.stream;

  @override
  Future<void> show(DesktopNotification notification) async {
    if (!_usable) return;
    try {
      final reply = await _client
          .callMethod(
            destination: _service,
            path: DBusObjectPath('/org/freedesktop/Notifications'),
            interface: _service,
            name: 'Notify',
            values: [
              const DBusString('Opencord'),
              const DBusUint32(0),
              const DBusString('dev.opencord.opencord'),
              DBusString(notification.title),
              DBusString(notification.body),
              DBusArray.string(['default', 'Open']),
              DBusDict.stringVariant({
                'desktop-entry': const DBusString('dev.opencord.opencord'),
                'category': const DBusString('im.received'),
                if (notification.sound)
                  'sound-name': const DBusString('message-new-instant')
                else
                  'suppress-sound': const DBusBoolean(true),
              }),
              const DBusInt32(-1),
            ],
            replySignature: DBusSignature('u'),
          )
          .timeout(_answerWithin);
      _links[reply.values.single.asUint32()] = notification.link;
    } on TimeoutException {
      _usable = false;
      developer.log('No notification daemon answered', name: 'notify');
    } on DBusMethodResponseException catch (error) {
      developer.log('A notification was refused: $error', name: 'notify');
    }
  }

  @override
  Future<void> close() async {
    for (final subscription in _subscriptions) {
      await subscription.cancel();
    }
    await _client.close();
    await _opened.close();
  }
}
