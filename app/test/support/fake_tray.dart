import 'dart:async';

import 'package:opencord/features/desktop/tray.dart';
import 'package:opencord/features/desktop/tray_menu.dart';

/// Records what the app shows in the tray, and clicks on its behalf.
class FakeTray implements TrayService {
  FakeTray({this.available = true});

  @override
  bool available;

  final shown = <TrayState>[];
  final _actions = StreamController<TrayAction>.broadcast();

  void click(TrayAction action) => _actions.add(action);

  @override
  Stream<TrayAction> get actions => _actions.stream;

  @override
  Future<void> show(TrayState state) async => shown.add(state);

  @override
  Future<void> close() => _actions.close();
}
