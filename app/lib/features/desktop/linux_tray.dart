import 'dart:async';
import 'dart:developer' as developer;
import 'dart:io';
import 'dart:typed_data';

import 'package:dbus/dbus.dart';

import 'package:opencord/features/desktop/dbus_menu.dart';
import 'package:opencord/features/desktop/status_notifier_item.dart';
import 'package:opencord/features/desktop/tray.dart';
import 'package:opencord/features/desktop/tray_icon.dart';
import 'package:opencord/features/desktop/tray_menu.dart';

const _watcher = 'org.kde.StatusNotifierWatcher';

/// The tray on Linux (§15): a StatusNotifierItem and its dbusmenu on the
/// session bus, announced to the desktop's StatusNotifierWatcher (Waybar on
/// Hyprland, KDE, GNOME with the AppIndicator extension). It announces
/// itself again when the watcher restarts.
class LinuxTray implements TrayService {
  LinuxTray._(this._client);

  final DBusClient _client;
  final _actions = StreamController<TrayAction>.broadcast();
  late final _menu = DBusMenu(onAction: _actions.add);
  late final _item = StatusNotifierItem(
    onActivate: () => _actions.add(const OpenWindow()),
  );
  final _name = 'org.kde.StatusNotifierItem-$pid-1';
  StreamSubscription<DBusNameOwnerChangedEvent>? _watching;
  var _available = false;
  TrayState? _shown;

  /// The tray on the session bus, or null when there is no bus.
  static Future<LinuxTray?> start() async {
    final DBusClient client;
    try {
      client = DBusClient.session();
    } on Exception catch (error) {
      developer.log('No session bus for the tray: $error', name: 'tray');
      return null;
    }
    final tray = LinuxTray._(client);
    try {
      await tray._start();
      return tray;
    } on Exception catch (error) {
      developer.log('The tray could not start: $error', name: 'tray');
      await client.close();
      return null;
    }
  }

  Future<void> _start() async {
    await _client.registerObject(_menu);
    await _client.registerObject(_item);
    await _client.requestName(
      _name,
      flags: const {DBusRequestNameFlag.doNotQueue},
    );
    _watching = _client.nameOwnerChanged
        .where((event) => event.name == _watcher)
        .listen((event) {
          if (event.newOwner == null) {
            _available = false;
          } else {
            unawaited(_announce());
          }
        });
    if (await _client.nameHasOwner(_watcher)) await _announce();
  }

  Future<void> _announce() async {
    try {
      await _client.callMethod(
        destination: _watcher,
        path: DBusObjectPath('/StatusNotifierWatcher'),
        interface: _watcher,
        name: 'RegisterStatusNotifierItem',
        values: [DBusString(_name)],
        replySignature: DBusSignature(''),
      );
      _available = true;
    } on DBusMethodResponseException catch (error) {
      developer.log('The tray watcher refused the icon: $error', name: 'tray');
      _available = false;
    }
  }

  @override
  bool get available => _available;

  @override
  Stream<TrayAction> get actions => _actions.stream;

  @override
  Future<void> show(TrayState state) async {
    final previous = _shown;
    _shown = state;
    final dot = state.unread > 0;
    if (previous == null ||
        previous.dark != state.dark ||
        (previous.unread > 0) != dot) {
      _item.icons = <int, Uint8List>{
        for (final size in trayIconSizes)
          size: await renderTrayIcon(size, dark: state.dark, dot: dot),
      };
    }
    _item.tooltip = state.tooltip;
    if (previous == null ||
        previous.muted != state.muted ||
        previous.deafened != state.deafened ||
        previous.presence != state.presence) {
      _menu.items = trayMenu(state);
    }
  }

  @override
  Future<void> close() async {
    await _watching?.cancel();
    await _client.close();
    await _actions.close();
  }
}
