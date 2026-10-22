import 'dart:typed_data';

import 'package:dbus/dbus.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/desktop/status_notifier_item.dart';

const _interface = 'org.kde.StatusNotifierItem';

Future<DBusValue> _property(StatusNotifierItem item, String name) async {
  final response = await item.getProperty(_interface, name);
  return (response as DBusMethodSuccessResponse).values.single.asVariant();
}

void main() {
  test('the item names the app and points at its menu', () async {
    final item = StatusNotifierItem(onActivate: () {});

    expect(await _property(item, 'Id'), const DBusString('opencord'));
    expect(await _property(item, 'Title'), const DBusString('Opencord'));
    expect(
      await _property(item, 'Category'),
      const DBusString('Communications'),
    );
    expect(await _property(item, 'Status'), const DBusString('Active'));
    expect(await _property(item, 'ItemIsMenu'), const DBusBoolean(false));
    expect(await _property(item, 'Menu'), DBusObjectPath('/MenuBar'));
  });

  test('icons are offered as pixmaps, one per size', () async {
    final item = StatusNotifierItem(onActivate: () {})
      ..icons = {16: Uint8List(16 * 16 * 4), 22: Uint8List(22 * 22 * 4)};

    final pixmaps = (await _property(item, 'IconPixmap')).asArray();

    expect(
      [
        for (final pixmap in pixmaps)
          (pixmap.asStruct()[0].asInt32(), pixmap.asStruct()[1].asInt32()),
      ],
      [(16, 16), (22, 22)],
    );
    expect(pixmaps.last.asStruct()[2].asByteArray(), hasLength(22 * 22 * 4));
  });

  test('the tooltip carries the unread count', () async {
    final item = StatusNotifierItem(onActivate: () {})
      ..tooltip = '3 unread messages';

    final [_, _, title, body] = (await _property(item, 'ToolTip')).asStruct();

    expect(title, const DBusString('Opencord'));
    expect(body, const DBusString('3 unread messages'));
  });

  test('a click on the icon opens the window', () async {
    var opened = 0;
    final item = StatusNotifierItem(onActivate: () => opened++);

    await item.handleMethodCall(
      const DBusMethodCall(
        sender: ':1.7',
        interface: _interface,
        name: 'Activate',
        values: [DBusInt32(10), DBusInt32(20)],
      ),
    );

    expect(opened, 1);
  });
}
