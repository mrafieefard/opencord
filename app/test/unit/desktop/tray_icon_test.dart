import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/desktop/tray_icon.dart';
import 'package:opencord/features/desktop/tray_menu.dart';

const _size = 32;

/// The alpha of the pixel at (x, y) of an ARGB icon.
int _alpha(Uint8List argb, int x, int y) => argb[(y * _size + x) * 4];

Future<Uint8List> _icon(TrayVoice voice, {bool dot = false}) =>
    renderTrayIcon(_size, dark: true, dot: dot, voice: voice);

void main() {
  test(
    'out of voice the icon is the chat bubble, hollow in the middle',
    () async {
      final icon = await _icon(TrayVoice.none);

      expect(_alpha(icon, 16, 15), 0);
      // The bubble's tail, bottom left.
      expect(_alpha(icon, 7, 26), greaterThan(200));
    },
  );

  test('in voice it is a ring, filled while talking', () async {
    final quiet = await _icon(TrayVoice.quiet);
    final speaking = await _icon(TrayVoice.speaking);

    expect(_alpha(quiet, 16, 16), 0);
    expect(_alpha(speaking, 16, 16), greaterThan(200));
    // No bubble tail in voice.
    expect(_alpha(quiet, 5, 29), 0);
  });

  test('muted, a slash crosses the ring', () async {
    final muted = await _icon(TrayVoice.muted);

    // The slash goes through the middle, where the quiet ring is empty.
    expect(_alpha(muted, 16, 16), greaterThan(200));
    expect(muted, isNot(equals(await _icon(TrayVoice.speaking))));
  });

  test('every voice state keeps the unread dot', () async {
    for (final voice in TrayVoice.values) {
      final icon = await _icon(voice, dot: true);
      expect(_alpha(icon, 26, 6), greaterThan(200), reason: '$voice');
    }
  });
}
