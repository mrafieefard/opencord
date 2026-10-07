import 'dart:async';

import 'package:dbus/dbus.dart';

import 'package:opencord/features/desktop/tray_menu.dart';

const _interface = 'com.canonical.dbusmenu';

/// The tray menu over `com.canonical.dbusmenu`, where StatusNotifierItem
/// hosts (Waybar, KDE, GNOME's AppIndicator extension) read it (§15).
class DBusMenu extends DBusObject {
  DBusMenu({required this.onAction}) : super(DBusObjectPath('/MenuBar'));

  final void Function(TrayAction action) onAction;

  var _items = const <TrayMenuItem>[];
  var _revision = 1;

  /// Replaces the menu and tells hosts it changed.
  set items(List<TrayMenuItem> items) {
    _items = items;
    _revision++;
    unawaited(
      emitSignal(_interface, 'LayoutUpdated', [
        DBusUint32(_revision),
        const DBusInt32(0),
      ]),
    );
  }

  TrayMenuItem? _find(int id, [List<TrayMenuItem>? items]) {
    for (final item in items ?? _items) {
      if (item.id == id) return item;
      if (_find(id, item.children) case final found?) return found;
    }
    return null;
  }

  Map<String, DBusValue> _properties(TrayMenuItem item) => {
    if (item.separator)
      'type': const DBusString('separator')
    // Underscores mark mnemonics in dbusmenu labels.
    else
      'label': DBusString(item.label.replaceAll('_', '__')),
    if (item.checked case final checked?) ...{
      'toggle-type': DBusString(item.radio ? 'radio' : 'checkmark'),
      'toggle-state': DBusInt32(checked ? 1 : 0),
    },
    if (item.children.isNotEmpty)
      'children-display': const DBusString('submenu'),
  };

  /// `(ia{sv}av)`: an item, its properties and [depth] levels of children
  /// (all of them when negative).
  DBusStruct _layout(
    int id,
    Map<String, DBusValue> properties,
    List<TrayMenuItem> children,
    int depth,
  ) => DBusStruct([
    DBusInt32(id),
    DBusDict.stringVariant(properties),
    DBusArray(DBusSignature('v'), [
      if (depth != 0)
        for (final child in children)
          DBusVariant(
            _layout(child.id, _properties(child), child.children, depth - 1),
          ),
    ]),
  ]);

  @override
  Future<DBusMethodResponse> handleMethodCall(DBusMethodCall methodCall) async {
    final call = methodCall;
    if (call.interface != _interface) {
      return DBusMethodErrorResponse.unknownInterface();
    }
    switch (call.name) {
      case 'GetLayout':
        final parent = call.values[0].asInt32();
        final depth = call.values[1].asInt32();
        if (parent == 0) {
          return DBusMethodSuccessResponse([
            DBusUint32(_revision),
            _layout(
              0,
              {'children-display': const DBusString('submenu')},
              _items,
              depth,
            ),
          ]);
        }
        final item = _find(parent);
        if (item == null) return _unknownItem(parent);
        return DBusMethodSuccessResponse([
          DBusUint32(_revision),
          _layout(item.id, _properties(item), item.children, depth),
        ]);
      case 'GetGroupProperties':
        return DBusMethodSuccessResponse([
          DBusArray(DBusSignature('(ia{sv})'), [
            for (final id in call.values[0].asInt32Array())
              if (_find(id) case final item?)
                DBusStruct([
                  DBusInt32(id),
                  DBusDict.stringVariant(_properties(item)),
                ]),
          ]),
        ]);
      case 'GetProperty':
        final id = call.values[0].asInt32();
        final item = _find(id);
        final value = item == null
            ? null
            : _properties(item)[call.values[1].asString()];
        if (value == null) return _unknownItem(id);
        return DBusMethodSuccessResponse([DBusVariant(value)]);
      case 'Event':
        _event(call.values[0].asInt32(), call.values[1].asString());
        return DBusMethodSuccessResponse();
      case 'EventGroup':
        for (final event in call.values[0].asArray()) {
          final [id, name, _, _] = event.asStruct();
          _event(id.asInt32(), name.asString());
        }
        return DBusMethodSuccessResponse([DBusArray.int32([])]);
      case 'AboutToShow':
        return DBusMethodSuccessResponse([const DBusBoolean(false)]);
      case 'AboutToShowGroup':
        return DBusMethodSuccessResponse([
          DBusArray.int32([]),
          DBusArray.int32([]),
        ]);
      default:
        return DBusMethodErrorResponse.unknownMethod();
    }
  }

  void _event(int id, String name) {
    if (name != 'clicked') return;
    if (_find(id)?.action case final action?) onAction(action);
  }

  DBusMethodErrorResponse _unknownItem(int id) => DBusMethodErrorResponse(
    'org.freedesktop.DBus.Error.InvalidArgs',
    [DBusString('No menu item $id')],
  );

  static final _menuProperties = <String, DBusValue>{
    'Version': const DBusUint32(3),
    'TextDirection': const DBusString('ltr'),
    'Status': const DBusString('normal'),
    'IconThemePath': DBusArray.string([]),
  };

  @override
  Future<DBusMethodResponse> getProperty(String interface, String name) async {
    final value = interface == _interface ? _menuProperties[name] : null;
    if (value == null) return DBusMethodErrorResponse.unknownProperty();
    return DBusGetPropertyResponse(value);
  }

  @override
  Future<DBusMethodResponse> getAllProperties(String interface) async =>
      DBusGetAllPropertiesResponse(
        interface == _interface ? _menuProperties : const {},
      );

  @override
  List<DBusIntrospectInterface> introspect() => [
    DBusIntrospectInterface(
      _interface,
      methods: [
        DBusIntrospectMethod(
          'GetLayout',
          args: [
            DBusIntrospectArgument(
              DBusSignature('i'),
              DBusArgumentDirection.in_,
              name: 'parentId',
            ),
            DBusIntrospectArgument(
              DBusSignature('i'),
              DBusArgumentDirection.in_,
              name: 'recursionDepth',
            ),
            DBusIntrospectArgument(
              DBusSignature('as'),
              DBusArgumentDirection.in_,
              name: 'propertyNames',
            ),
            DBusIntrospectArgument(
              DBusSignature('u'),
              DBusArgumentDirection.out,
              name: 'revision',
            ),
            DBusIntrospectArgument(
              DBusSignature('(ia{sv}av)'),
              DBusArgumentDirection.out,
              name: 'layout',
            ),
          ],
        ),
        DBusIntrospectMethod(
          'GetGroupProperties',
          args: [
            DBusIntrospectArgument(
              DBusSignature('ai'),
              DBusArgumentDirection.in_,
              name: 'ids',
            ),
            DBusIntrospectArgument(
              DBusSignature('as'),
              DBusArgumentDirection.in_,
              name: 'propertyNames',
            ),
            DBusIntrospectArgument(
              DBusSignature('a(ia{sv})'),
              DBusArgumentDirection.out,
              name: 'properties',
            ),
          ],
        ),
        DBusIntrospectMethod(
          'GetProperty',
          args: [
            DBusIntrospectArgument(
              DBusSignature('i'),
              DBusArgumentDirection.in_,
              name: 'id',
            ),
            DBusIntrospectArgument(
              DBusSignature('s'),
              DBusArgumentDirection.in_,
              name: 'name',
            ),
            DBusIntrospectArgument(
              DBusSignature('v'),
              DBusArgumentDirection.out,
              name: 'value',
            ),
          ],
        ),
        DBusIntrospectMethod(
          'Event',
          args: [
            DBusIntrospectArgument(
              DBusSignature('i'),
              DBusArgumentDirection.in_,
              name: 'id',
            ),
            DBusIntrospectArgument(
              DBusSignature('s'),
              DBusArgumentDirection.in_,
              name: 'eventId',
            ),
            DBusIntrospectArgument(
              DBusSignature('v'),
              DBusArgumentDirection.in_,
              name: 'data',
            ),
            DBusIntrospectArgument(
              DBusSignature('u'),
              DBusArgumentDirection.in_,
              name: 'timestamp',
            ),
          ],
        ),
        DBusIntrospectMethod(
          'EventGroup',
          args: [
            DBusIntrospectArgument(
              DBusSignature('a(isvu)'),
              DBusArgumentDirection.in_,
              name: 'events',
            ),
            DBusIntrospectArgument(
              DBusSignature('ai'),
              DBusArgumentDirection.out,
              name: 'idErrors',
            ),
          ],
        ),
        DBusIntrospectMethod(
          'AboutToShow',
          args: [
            DBusIntrospectArgument(
              DBusSignature('i'),
              DBusArgumentDirection.in_,
              name: 'id',
            ),
            DBusIntrospectArgument(
              DBusSignature('b'),
              DBusArgumentDirection.out,
              name: 'needUpdate',
            ),
          ],
        ),
        DBusIntrospectMethod(
          'AboutToShowGroup',
          args: [
            DBusIntrospectArgument(
              DBusSignature('ai'),
              DBusArgumentDirection.in_,
              name: 'ids',
            ),
            DBusIntrospectArgument(
              DBusSignature('ai'),
              DBusArgumentDirection.out,
              name: 'updatesNeeded',
            ),
            DBusIntrospectArgument(
              DBusSignature('ai'),
              DBusArgumentDirection.out,
              name: 'idErrors',
            ),
          ],
        ),
      ],
      signals: [
        DBusIntrospectSignal(
          'LayoutUpdated',
          args: [
            DBusIntrospectArgument(
              DBusSignature('u'),
              DBusArgumentDirection.out,
              name: 'revision',
            ),
            DBusIntrospectArgument(
              DBusSignature('i'),
              DBusArgumentDirection.out,
              name: 'parent',
            ),
          ],
        ),
      ],
      properties: [
        DBusIntrospectProperty(
          'Version',
          DBusSignature('u'),
          access: DBusPropertyAccess.read,
        ),
        DBusIntrospectProperty(
          'TextDirection',
          DBusSignature('s'),
          access: DBusPropertyAccess.read,
        ),
        DBusIntrospectProperty(
          'Status',
          DBusSignature('s'),
          access: DBusPropertyAccess.read,
        ),
        DBusIntrospectProperty(
          'IconThemePath',
          DBusSignature('as'),
          access: DBusPropertyAccess.read,
        ),
      ],
    ),
  ];
}
