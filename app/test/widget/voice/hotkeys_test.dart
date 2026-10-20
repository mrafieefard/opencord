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
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.pushToTalk, keys: 'grave');
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
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.toggleMute, keys: 'ALT+F12');
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
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.toggleDeafen, keys: 'F12');
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

  testWidgets('focused, push to talk on two keys lasts until both are up', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app.read(keybindsProvider.notifier)
      ..add(HotkeyAction.pushToTalk, keys: 'grave')
      ..add(HotkeyAction.pushToTalk, keys: 'Control_R');
    await _settle(tester);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.backquote);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlRight);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.backquote);
    final oneUp = app.repository.pushToTalkHeld;
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlRight);

    expect(oneUp, isTrue, reason: 'Right Ctrl still holds it');
    expect(app.repository.pushToTalkHeld, isFalse);
    await app.dispose(tester);
  });

  testWidgets('Toggle push-to-talk switches the input mode', (tester) async {
    final app = await MockApp.pump(tester);
    app
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.toggleInputMode, keys: 'F9');
    await _settle(tester);
    final before = app.read(audioSettingsProvider).inputMode;

    await tester.sendKeyEvent(LogicalKeyboardKey.f9);
    final switched = app.read(audioSettingsProvider).inputMode;
    // Pressed elsewhere, as the system reports it.
    app.repository.debugHotkeyPressed(HotkeyAction.toggleInputMode);
    await _settle(tester);

    expect(before, InputMode.voiceActivity);
    expect(switched, InputMode.pushToTalk);
    expect(app.read(audioSettingsProvider).inputMode, InputMode.voiceActivity);
    await app.dispose(tester);
  });

  testWidgets('toggle music mute mutes and unmutes the music', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.toggleMusicMute, keys: 'F8');
    await _settle(tester);

    await tester.sendKeyEvent(LogicalKeyboardKey.f8);
    final muted = app.read(musicListeningProvider).muted;
    // Pressed elsewhere, as the system reports it.
    app.repository.debugHotkeyPressed(HotkeyAction.toggleMusicMute);
    await _settle(tester);

    expect(muted, isTrue);
    expect(app.read(musicListeningProvider).muted, isFalse);
    await app.dispose(tester);
  });

  testWidgets("a keybind on the app's own toggle keys acts once", (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    app
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.toggleMute, keys: 'CTRL+SHIFT+m');
    await _settle(tester);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyM);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await _settle(tester);

    expect(app.read(voiceSessionProvider).muted, isTrue);
    await app.dispose(tester);
  });
}
