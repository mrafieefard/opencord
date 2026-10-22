import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/window/window_startup.dart';

void main() {
  final minimized = AppSettings.defaults(
    TargetPlatform.linux,
  ).copyWith(startMinimized: true);

  test('Start minimized applies to a start at login, not a launch by hand', () {
    expect(
      startsHidden(atLogin: true, settings: minimized, tray: true),
      isTrue,
    );
    // Clicked in the launcher: the window the person asked for.
    expect(
      startsHidden(atLogin: false, settings: minimized, tray: true),
      isFalse,
    );
  });

  test('never hidden without a tray to come back from, or with it off', () {
    expect(
      startsHidden(atLogin: true, settings: minimized, tray: false),
      isFalse,
    );
    expect(
      startsHidden(
        atLogin: true,
        settings: minimized.copyWith(startMinimized: false),
        tray: true,
      ),
      isFalse,
    );
  });
}
