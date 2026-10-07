import 'package:dbus/dbus.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/desktop/dbus_menu.dart';
import 'package:opencord/features/desktop/tray_menu.dart';

const _interface = 'com.canonical.dbusmenu';

Future<List<DBusValue>> _call(
  DBusMenu menu,
  String name, [
  List<DBusValue> values = const [],
]) async {
  final response = await menu.handleMethodCall(
    DBusMethodCall(
      sender: ':1.42',
      interface: _interface,
      name: name,
      values: values,
    ),
  );
  return (response as DBusMethodSuccessResponse).values;
}

/// A layout node as `{id, props, children}`.
({int id, Map<String, DBusValue> props, List<Object> children}) _node(
  DBusValue value,
) {
  final [id, props, children] = value.asStruct();
  return (
    id: id.asInt32(),
    props: props.asStringVariantDict(),
    children: [for (final child in children.asVariantArray()) _node(child)],
  );
}

Future<({int id, Map<String, DBusValue> props, List<Object> children})> _layout(
  DBusMenu menu,
) async {
  final [_, layout] = await _call(menu, 'GetLayout', [
    const DBusInt32(0),
    const DBusInt32(-1),
    DBusArray.string([]),
  ]);
  return _node(layout);
}

void main() {
  test('the layout is the tray menu, with submenus and toggles', () async {
    final menu = DBusMenu(onAction: (_) {})
      ..items = trayMenu(const TrayState(muted: true));

    final root = await _layout(menu);
    final children = root.children.cast<dynamic>();

    expect(root.id, 0);
    expect(root.props['children-display'], const DBusString('submenu'));
    expect(children, hasLength(7));
    final mute = children[2];
    expect(mute.props['label'], const DBusString('Mute'));
    expect(mute.props['toggle-type'], const DBusString('checkmark'));
    expect(mute.props['toggle-state'], const DBusInt32(1));
    expect(children[1].props['type'], const DBusString('separator'));
    final status = children[4];
    expect(status.props['children-display'], const DBusString('submenu'));
    expect(status.children, hasLength(SelfPresence.values.length));
    expect(
      (status.children.first as dynamic).props['toggle-type'],
      const DBusString('radio'),
    );
  });

  test('a click runs the item’s action', () async {
    final actions = <TrayAction>[];
    final menu = DBusMenu(onAction: actions.add)
      ..items = trayMenu(const TrayState());

    await _call(menu, 'Event', [
      const DBusInt32(4),
      const DBusString('clicked'),
      const DBusVariant(DBusInt32(0)),
      const DBusUint32(0),
    ]);
    await _call(menu, 'Event', [
      const DBusInt32(12),
      const DBusString('hovered'),
      const DBusVariant(DBusInt32(0)),
      const DBusUint32(0),
    ]);

    expect(actions, [const ToggleDeafen()]);
  });

  test('a new menu gets a new revision', () async {
    final menu = DBusMenu(onAction: (_) {})
      ..items = trayMenu(const TrayState());
    final [before, _] = await _call(menu, 'GetLayout', [
      const DBusInt32(0),
      const DBusInt32(-1),
      DBusArray.string([]),
    ]);

    menu.items = trayMenu(const TrayState(deafened: true));
    final [after, _] = await _call(menu, 'GetLayout', [
      const DBusInt32(0),
      const DBusInt32(-1),
      DBusArray.string([]),
    ]);

    expect(after.asUint32(), greaterThan(before.asUint32()));
  });

  test('group properties and the version are answered', () async {
    final menu = DBusMenu(onAction: (_) {})
      ..items = trayMenu(const TrayState());

    final [group] = await _call(menu, 'GetGroupProperties', [
      DBusArray.int32([1, 7]),
      DBusArray.string([]),
    ]);
    final labels = [
      for (final entry in group.asArray())
        entry.asStruct()[1].asStringVariantDict()['label'],
    ];
    final version = await menu.getProperty(_interface, 'Version');

    expect(labels, [
      const DBusString('Open Opencord'),
      const DBusString('Quit Opencord'),
    ]);
    expect(
      (version as DBusMethodSuccessResponse).values.single,
      const DBusVariant(DBusUint32(3)),
    );
  });
}
