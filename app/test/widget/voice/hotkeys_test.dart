import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';

import '../../support/app.dart';

/// Repository answers arrive in a microtask; the frame after shows them.
Future<void> _settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump();
}

void main() {
  testWidgets('focused, the push-to-talk hotkey holds push-to-talk', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app
        .read(hotkeyBindingsProvider.notifier)
        .bind(HotkeyAction.pushToTalk, 'grave');
    await _settle(tester);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.backquote);
    final held = app.repository.pushToTalkHeld;
    await tester.sendKeyUpEvent(LogicalKeyboardKey.backquote);

    expect(app.read(hotkeySupportProvider), isA<HotkeysFocusedOnly>());
    expect(held, isTrue);
    expect(app.repository.pushToTalkHeld, isFalse);
    await app.dispose(tester);
  });

  testWidgets('focused, a toggle hotkey mutes, with its modifiers only', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app
        .read(hotkeyBindingsProvider.notifier)
        .bind(HotkeyAction.toggleMute, 'ALT+F12');
    await _settle(tester);

    await tester.sendKeyEvent(LogicalKeyboardKey.f12);
    final withoutAlt = app.read(voiceSessionProvider).muted;
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.f12);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await _settle(tester);

    expect(withoutAlt, isFalse);
    expect(app.read(voiceSessionProvider).muted, isTrue);
    await app.dispose(tester);
  });

  testWidgets('bound system-wide, keys in the app do not act a second time', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app.repository.hotkeySupport = const HotkeySupport.global('the portal');
    app
        .read(hotkeyBindingsProvider.notifier)
        .bind(HotkeyAction.toggleDeafen, 'F12');
    await _settle(tester);

    await tester.sendKeyEvent(LogicalKeyboardKey.f12);
    await _settle(tester);
    final fromTheApp = app.read(voiceSessionProvider).deafened;
    app.repository.debugHotkeyPressed(HotkeyAction.toggleDeafen);
    await _settle(tester);

    expect(fromTheApp, isFalse);
    expect(app.read(voiceSessionProvider).deafened, isTrue);
    await app.dispose(tester);
  });
}
