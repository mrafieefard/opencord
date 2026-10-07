import 'dart:io';

import 'package:dbus/dbus.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/desktop/linux_notifications.dart';
import 'package:opencord/features/desktop/notifications.dart';

/// A notification daemon that keeps what it was asked to show.
class _Daemon extends DBusObject {
  _Daemon({required this.markup})
    : super(DBusObjectPath('/org/freedesktop/Notifications'));

  final bool markup;
  final bodies = <String>[];

  @override
  Future<DBusMethodResponse> handleMethodCall(DBusMethodCall methodCall) async {
    switch (methodCall.name) {
      case 'GetCapabilities':
        return DBusMethodSuccessResponse([
          DBusArray.string(['body', 'actions', if (markup) 'body-markup']),
        ]);
      case 'Notify':
        bodies.add(methodCall.values[4].asString());
        return DBusMethodSuccessResponse([const DBusUint32(1)]);
    }
    return DBusMethodErrorResponse.unknownMethod();
  }
}

/// A bus of our own with [daemon] on it, so the desktop's is left alone.
Future<LinuxNotifications> _on(_Daemon daemon) async {
  final dir = await Directory.systemTemp.createTemp('opencord-bus');
  final server = DBusServer();
  final address = await server.listenAddress(DBusAddress.unix(dir: dir));
  final side = DBusClient(address);
  await side.requestName('org.freedesktop.Notifications');
  await side.registerObject(daemon);
  final notifications = LinuxNotifications.startOn(DBusClient(address));
  addTearDown(() async {
    await notifications.close();
    await side.close();
    await server.close();
    await dir.delete(recursive: true);
  });
  return notifications;
}

const _said = DesktopNotification(
  title: 'Kai (#general, Home)',
  body: 'if a<b && c > d, <b>read this</b>',
  link: 'opencord://home.example:7710/c/10/5',
);

void main() {
  test('what people wrote is not taken as markup', () async {
    final daemon = _Daemon(markup: true);
    final notifications = await _on(daemon);

    await notifications.show(_said);

    expect(daemon.bodies, [
      'if a&lt;b &amp;&amp; c &gt; d, &lt;b&gt;read this&lt;/b&gt;',
    ]);
  });

  test('a daemon without markup gets the text as written', () async {
    final daemon = _Daemon(markup: false);
    final notifications = await _on(daemon);

    await notifications.show(_said);

    expect(daemon.bodies, [_said.body]);
  });
}
