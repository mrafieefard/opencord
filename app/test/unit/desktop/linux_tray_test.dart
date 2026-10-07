import 'dart:async';
import 'dart:io';

import 'package:dbus/dbus.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/desktop/linux_tray.dart';

/// A tray watcher that takes the call and never answers: a hung panel.
class _StuckWatcher extends DBusObject {
  _StuckWatcher() : super(DBusObjectPath('/StatusNotifierWatcher'));

  @override
  Future<DBusMethodResponse> handleMethodCall(DBusMethodCall methodCall) =>
      Completer<DBusMethodResponse>().future;
}

void main() {
  test(
    'a tray watcher that never answers does not hold up the start',
    () async {
      // A bus of our own, so the desktop's is left alone.
      final dir = await Directory.systemTemp.createTemp('opencord-bus');
      final server = DBusServer();
      final address = await server.listenAddress(DBusAddress.unix(dir: dir));
      final watcher = DBusClient(address);
      await watcher.requestName('org.kde.StatusNotifierWatcher');
      await watcher.registerObject(_StuckWatcher());
      final client = DBusClient(address);
      addTearDown(() async {
        await client.close();
        await watcher.close();
        await server.close();
        await dir.delete(recursive: true);
      });

      final tray = await LinuxTray.startOn(
        client,
        answerWithin: const Duration(milliseconds: 200),
      ).timeout(const Duration(seconds: 5));

      expect(tray, isNotNull);
      expect(tray!.available, isFalse);
    },
  );
}
