import 'dart:async';
import 'dart:typed_data';

import 'package:dbus/dbus.dart';

const _interface = 'org.kde.StatusNotifierItem';

/// The tray icon as a StatusNotifierItem (§15): what Waybar, KDE and
/// GNOME's AppIndicator extension show. Its menu is a separate object.
class StatusNotifierItem extends DBusObject {
  StatusNotifierItem({required this.onActivate})
    : super(DBusObjectPath('/StatusNotifierItem'));

  /// A click on the icon.
  final void Function() onActivate;

  var _icons = const <int, Uint8List>{};
  var _tooltip = '';

  /// ARGB32 pixmaps by size.
  set icons(Map<int, Uint8List> icons) {
    _icons = icons;
    unawaited(emitSignal(_interface, 'NewIcon'));
  }

  set tooltip(String tooltip) {
    if (tooltip == _tooltip) return;
    _tooltip = tooltip;
    unawaited(emitSignal(_interface, 'NewToolTip'));
  }

  static final _pixmapSignature = DBusSignature('(iiay)');

  DBusArray _pixmaps(Map<int, Uint8List> icons) => DBusArray(_pixmapSignature, [
    for (final MapEntry(key: size, value: pixels) in icons.entries)
      DBusStruct([DBusInt32(size), DBusInt32(size), DBusArray.byte(pixels)]),
  ]);

  Map<String, DBusValue> get _properties => {
    'Category': const DBusString('Communications'),
    'Id': const DBusString('opencord'),
    'Title': const DBusString('Opencord'),
    'Status': const DBusString('Active'),
    'WindowId': const DBusInt32(0),
    'IconName': const DBusString(''),
    'IconPixmap': _pixmaps(_icons),
    'OverlayIconName': const DBusString(''),
    'OverlayIconPixmap': _pixmaps(const {}),
    'AttentionIconName': const DBusString(''),
    'AttentionIconPixmap': _pixmaps(const {}),
    'AttentionMovieName': const DBusString(''),
    'ToolTip': DBusStruct([
      const DBusString(''),
      _pixmaps(const {}),
      const DBusString('Opencord'),
      DBusString(_tooltip),
    ]),
    'ItemIsMenu': const DBusBoolean(false),
    'Menu': DBusObjectPath('/MenuBar'),
    'IconThemePath': const DBusString(''),
  };

  @override
  Future<DBusMethodResponse> handleMethodCall(DBusMethodCall methodCall) async {
    if (methodCall.interface != _interface) {
      return DBusMethodErrorResponse.unknownInterface();
    }
    switch (methodCall.name) {
      case 'Activate':
      case 'SecondaryActivate':
        onActivate();
        return DBusMethodSuccessResponse();
      case 'ContextMenu':
      case 'Scroll':
      case 'ProvideXdgActivationToken':
        // The host shows the menu itself; scrolling means nothing here.
        return DBusMethodSuccessResponse();
      default:
        return DBusMethodErrorResponse.unknownMethod();
    }
  }

  @override
  Future<DBusMethodResponse> getProperty(String interface, String name) async {
    final value = interface == _interface ? _properties[name] : null;
    if (value == null) return DBusMethodErrorResponse.unknownProperty();
    return DBusGetPropertyResponse(value);
  }

  @override
  Future<DBusMethodResponse> getAllProperties(String interface) async =>
      DBusGetAllPropertiesResponse(
        interface == _interface ? _properties : const {},
      );

  @override
  List<DBusIntrospectInterface> introspect() {
    DBusIntrospectArgument int32In(String name) => DBusIntrospectArgument(
      DBusSignature('i'),
      DBusArgumentDirection.in_,
      name: name,
    );
    DBusIntrospectProperty read(String name, String type) =>
        DBusIntrospectProperty(
          name,
          DBusSignature(type),
          access: DBusPropertyAccess.read,
        );
    return [
      DBusIntrospectInterface(
        _interface,
        methods: [
          DBusIntrospectMethod('Activate', args: [int32In('x'), int32In('y')]),
          DBusIntrospectMethod(
            'SecondaryActivate',
            args: [int32In('x'), int32In('y')],
          ),
          DBusIntrospectMethod(
            'ContextMenu',
            args: [int32In('x'), int32In('y')],
          ),
          DBusIntrospectMethod(
            'Scroll',
            args: [
              int32In('delta'),
              DBusIntrospectArgument(
                DBusSignature('s'),
                DBusArgumentDirection.in_,
                name: 'orientation',
              ),
            ],
          ),
        ],
        signals: [
          DBusIntrospectSignal('NewIcon'),
          DBusIntrospectSignal('NewToolTip'),
          DBusIntrospectSignal(
            'NewStatus',
            args: [
              DBusIntrospectArgument(
                DBusSignature('s'),
                DBusArgumentDirection.out,
                name: 'status',
              ),
            ],
          ),
        ],
        properties: [
          read('Category', 's'),
          read('Id', 's'),
          read('Title', 's'),
          read('Status', 's'),
          read('WindowId', 'i'),
          read('IconName', 's'),
          read('IconPixmap', 'a(iiay)'),
          read('OverlayIconName', 's'),
          read('OverlayIconPixmap', 'a(iiay)'),
          read('AttentionIconName', 's'),
          read('AttentionIconPixmap', 'a(iiay)'),
          read('AttentionMovieName', 's'),
          read('ToolTip', '(sa(iiay)ss)'),
          read('ItemIsMenu', 'b'),
          read('Menu', 'o'),
          read('IconThemePath', 's'),
        ],
      ),
    ];
  }
}
