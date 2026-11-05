import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/features/settings/user/settings_draft.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/chat/links.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/settings/user/keybind_recorder.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';
import '../../support/fake_window.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inPage(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

Future<void> _openPage(WidgetTester tester, String page) async {
  await tester.tap(
    find.byWidgetPredicate(
      (widget) => widget is OcIconButton && widget.tooltip == 'User settings',
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(_inPage(find.text(page)));
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

Future<void> _addKeybind(WidgetTester tester) async {
  await tester.tap(_inPage(find.text('Add a keybind')));
  await _pumpFor(tester, const Duration(milliseconds: 100));
}

/// Keybinds wait for the settings' save bar, as in Discord.
Future<void> _saveChanges(WidgetTester tester) async {
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(
    find.descendant(
      of: find.byType(SaveBar),
      matching: find.widgetWithText(OcButton, 'Save changes'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

/// The keybinds as saved: (action, keys) in order.
List<(HotkeyAction, String)> _saved(MockApp app) => [
  for (final row in app.read(keybindsProvider).list) (row.action, row.keys),
];

void main() {
  testWidgets('a keybind is added, recorded from the keys pressed, and bound', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openPage(tester, 'Keybinds');
    expect(_inPage(find.text('No keybinds yet')), findsOneWidget);

    await _addKeybind(tester);
    // A new keybind listens at once, like Discord's.
    expect(_inPage(find.text('Press keys or a mouse button…')), findsOneWidget);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();
    expect(_inPage(find.text('Left Ctrl + Left Shift + …')), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await _pumpFor(tester, const Duration(milliseconds: 100));
    expect(_saved(app), isEmpty, reason: 'not before Save changes');
    expect(app.repository.hotkeys, isEmpty);
    await _saveChanges(tester);

    expect(_saved(app), [(HotkeyAction.pushToTalk, 'CTRL+SHIFT+k')]);
    expect(_inPage(find.text('Ctrl + Shift + K')), findsOneWidget);
    expect(app.repository.hotkeys, [
      (action: HotkeyAction.pushToTalk, keys: 'CTRL+SHIFT+k'),
    ]);

    // Another action for it.
    await tester.tap(_inPage(find.text('Push to talk')));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(
      find.descendant(
        of: find.byType(OcMenuPanel),
        matching: find.text('Toggle deafen'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await _saveChanges(tester);
    expect(_saved(app), [(HotkeyAction.toggleDeafen, 'CTRL+SHIFT+k')]);
    expect(app.repository.hotkeys.single.action, HotkeyAction.toggleDeafen);

    await tester.tap(_inPage(find.byTooltip('Remove keybind')));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await _saveChanges(tester);
    expect(_saved(app), isEmpty);
    expect(app.repository.hotkeys, isEmpty);
    await app.dispose(tester);
  });

  testWidgets('push to talk can have several keys, a modifier alone too', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openPage(tester, 'Keybinds');

    await _addKeybind(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.backquote);
    await _pumpFor(tester, const Duration(milliseconds: 100));
    await _addKeybind(tester);
    // Right Ctrl, pressed and let go with nothing else.
    await tester.sendKeyEvent(LogicalKeyboardKey.controlRight);
    await _pumpFor(tester, const Duration(milliseconds: 100));
    await _saveChanges(tester);

    expect(_saved(app), [
      (HotkeyAction.pushToTalk, 'grave'),
      (HotkeyAction.pushToTalk, 'Control_R'),
    ]);
    expect(_inPage(find.text('Right Ctrl')), findsOneWidget);
    expect(_inPage(find.text('Backtick')), findsOneWidget);
    expect(app.repository.hotkeys, hasLength(2));
    await app.dispose(tester);
  });

  testWidgets('recording: Escape stops, a key no hotkey can use says so', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    app
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.toggleMute, keys: 'F12');
    await _openPage(tester, 'Keybinds');

    await tester.tap(_inPage(find.text('F12')));
    await tester.pump();
    expect(app.read(recordingKeybindProvider), isTrue);
    await tester.sendKeyEvent(LogicalKeyboardKey.numpadEqual);
    await tester.pump();
    expect(_inPage(find.textContaining("can't be a keybind")), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();

    expect(app.read(recordingKeybindProvider), isFalse);
    expect(_saved(app), [(HotkeyAction.toggleMute, 'F12')]);
    expect(_inPage(find.text('F12')), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('keys pressed while recording do not act as hotkeys', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    app
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.pushToTalk, keys: 'grave');
    await _openPage(tester, 'Keybinds');

    await _addKeybind(tester);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.backquote);
    final heldWhileRecording = app.repository.pushToTalkHeld;
    await tester.sendKeyUpEvent(LogicalKeyboardKey.backquote);
    await _pumpFor(tester, const Duration(milliseconds: 100));

    expect(heldWhileRecording, isFalse);
    expect(_saved(app).last, (HotkeyAction.pushToTalk, 'grave'));
    await app.dispose(tester);
  });

  testWidgets('voice settings record the push-to-talk key and its delay', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openPage(tester, 'Voice & audio');
    expect(_inPage(find.text('Release delay')), findsNothing);
    await tester.tap(_inPage(find.text('Push to talk')));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    await tester.tap(_inPage(find.byType(KeybindRecorder)));
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.f13);
    await _pumpFor(tester, const Duration(milliseconds: 100));
    tester
        .widget<Slider>(
          find
              .descendant(
                of: find.ancestor(
                  of: _inPage(find.text('Release delay')),
                  matching: find.byType(Column),
                ),
                matching: find.byType(Slider),
              )
              .first,
        )
        .onChanged!(500);
    await tester.pump();
    await _saveChanges(tester);

    expect(_saved(app), [(HotkeyAction.pushToTalk, 'F13')]);
    expect(_inPage(find.text('500 ms')), findsOneWidget);
    expect(
      app.read(audioSettingsProvider).pushToTalkRelease,
      const Duration(milliseconds: 500),
    );
    await app.dispose(tester);
  });

  testWidgets(
    'keybinds waiting for a permission open its settings, and bind again when '
    'the window comes back',
    (tester) async {
      const url =
          'x-apple.systempreferences:com.apple.preference.security'
          '?Privacy_ListenEvent';
      const waiting = HotkeySupport.focusedOnly(
        'allow Opencord under Input Monitoring',
        settingsUrl: url,
      );
      final window = FakeNativeWindow();
      final opened = <Uri>[];
      final app = await MockApp.pump(
        tester,
        platform: TargetPlatform.macOS,
        window: window,
        overrides: [
          linkLauncherProvider.overrideWithValue((uri) async {
            opened.add(uri);
            return true;
          }),
        ],
      );
      app.repository.hotkeySupport = waiting;
      app.read(hotkeySupportProvider.notifier).set(waiting);
      await _openPage(tester, 'Keybinds');

      await tester.tap(_inPage(find.text('Open settings')));
      await tester.pump();
      expect(opened, [Uri.parse(url)]);

      // Allowed there, then back: the hotkeys are bound again, globally now.
      app.repository.hotkeySupport = const HotkeySupport.global('an event tap');
      final binds = app.repository.hotkeyBinds;
      window.statusEvents.add(const WindowStatus(focused: false));
      await tester.pump();
      window.statusEvents.add(const WindowStatus());
      await _pumpFor(tester, const Duration(milliseconds: 100));
      expect(app.repository.hotkeyBinds, binds + 1);
      expect(_inPage(find.text('Open settings')), findsNothing);

      // Bound: focus no longer binds them again.
      window.statusEvents.add(const WindowStatus(focused: false));
      await tester.pump();
      window.statusEvents.add(const WindowStatus());
      await _pumpFor(tester, const Duration(milliseconds: 100));
      expect(app.repository.hotkeyBinds, binds + 1);
      await app.dispose(tester);
    },
  );

  testWidgets('the_empty_row_mentions_the_mouse', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openPage(tester, 'Keybinds');
    expect(
      _inPage(
        find.text(
          'Add one to push to talk, mute or deafen with a key or mouse button, '
          'even from another app.',
        ),
      ),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  testWidgets('the_recorder_asks_for_either', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openPage(tester, 'Keybinds');
    await _addKeybind(tester);
    expect(_inPage(find.text('Press keys or a mouse button…')), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets(
    'recording_takes_a_mouse_button: each of the three, anywhere in the window; the row shows its label and the draft holds Mouse4',
    (tester) async {
      final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
      await _openPage(tester, 'Keybinds');

      for (final (buttonMask, text, label) in [
        (kMiddleMouseButton, 'Mouse3', 'Middle Mouse'),
        (kBackMouseButton, 'Mouse4', 'Mouse 4'),
        (kForwardMouseButton, 'Mouse5', 'Mouse 5'),
      ]) {
        await _addKeybind(tester);
        expect(
          _inPage(find.text('Press keys or a mouse button…')),
          findsOneWidget,
        );
        final gesture = await tester.createGesture(
          kind: PointerDeviceKind.mouse,
          buttons: buttonMask,
        );
        await gesture.down(const Offset(100, 100));
        await tester.pump();
        await gesture.up();
        await gesture.removePointer();
        await _pumpFor(tester, const Duration(milliseconds: 100));

        expect(_inPage(find.text(label)), findsOneWidget);
        expect(_saved(app), isEmpty); // Draft holds it, not saved yet
        final draftRow = app.read(shownKeybindsProvider).list.last;
        expect(draftRow.keys, text);

        // Remove row before next iteration
        await tester.tap(
          _inPage(find.byTooltip('Remove keybind')).last,
          warnIfMissed: false,
        );
        await _pumpFor(tester, const Duration(milliseconds: 100));
      }
      await app.dispose(tester);
    },
  );

  testWidgets(
    'modifiers_go_with_the_button: Ctrl held, Mouse 4: CTRL+Mouse4, Ctrl + Mouse 4',
    (tester) async {
      final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
      await _openPage(tester, 'Keybinds');
      await _addKeybind(tester);

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.pump();
      expect(_inPage(find.text('Left Ctrl + …')), findsOneWidget);

      final gesture = await tester.createGesture(
        kind: PointerDeviceKind.mouse,
        buttons: kBackMouseButton,
      );
      await gesture.down(const Offset(150, 150));
      await tester.pump();
      await gesture.up();
      await gesture.removePointer();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await _pumpFor(tester, const Duration(milliseconds: 100));

      expect(_inPage(find.text('Ctrl + Mouse 4')), findsOneWidget);
      final draftRow = app.read(shownKeybindsProvider).list.last;
      expect(draftRow.keys, 'CTRL+Mouse4');
      await app.dispose(tester);
    },
  );

  testWidgets(
    'left_click_on_the_recorder_stops_it_once: it stops and doesn\'t start again. Left click elsewhere and right click anywhere stop it with no change',
    (tester) async {
      final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
      await _openPage(tester, 'Keybinds');
      await _addKeybind(tester);
      expect(app.read(recordingKeybindProvider), isTrue);

      // Left click on the recorder stops it once and doesn't start again
      final recorderFinder = _inPage(find.byType(KeybindRecorder));
      await tester.tap(recorderFinder);
      await _pumpFor(tester, const Duration(milliseconds: 100));
      expect(app.read(recordingKeybindProvider), isFalse);

      // Tap on recorder starts it
      await tester.tap(recorderFinder);
      await _pumpFor(tester, const Duration(milliseconds: 100));
      expect(app.read(recordingKeybindProvider), isTrue);

      // Left click elsewhere (e.g. on Keybinds title) stops with no change
      final titlePos = tester.getCenter(_inPage(find.text('Keybinds')).first);
      final gesture = await tester.createGesture(
        kind: PointerDeviceKind.mouse,
        buttons: kPrimaryMouseButton,
      );
      await gesture.down(titlePos);
      await tester.pump();
      await gesture.up();
      await gesture.removePointer();
      await _pumpFor(tester, const Duration(milliseconds: 100));
      expect(app.read(recordingKeybindProvider), isFalse);

      // Start recording again
      await tester.tap(recorderFinder);
      await _pumpFor(tester, const Duration(milliseconds: 100));
      expect(app.read(recordingKeybindProvider), isTrue);

      // Right click anywhere stops with no change
      final rightClickGesture = await tester.createGesture(
        kind: PointerDeviceKind.mouse,
        buttons: kSecondaryMouseButton,
      );
      await rightClickGesture.down(const Offset(10, 10));
      await tester.pump();
      await rightClickGesture.up();
      await rightClickGesture.removePointer();
      await _pumpFor(tester, const Duration(milliseconds: 100));
      expect(app.read(recordingKeybindProvider), isFalse);

      await app.dispose(tester);
    },
  );

  testWidgets(
    'the_footer_says_where_mouse_works: only with mouse: false and a mouse keybind',
    (tester) async {
      final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
      app.repository.hotkeySupport = const HotkeySupport.global(
        'the portal',
        mouse: false,
      );
      app
          .read(hotkeySupportProvider.notifier)
          .set(const HotkeySupport.global('the portal', mouse: false));
      await _openPage(tester, 'Keybinds');

      // Without a mouse keybind: no sentence about mouse buttons
      expect(
        _inPage(
          find.text('Keybinds work in other apps too, through the portal.'),
        ),
        findsOneWidget,
      );
      expect(
        _inPage(
          find.textContaining(
            'Mouse buttons work only while Opencord is focused on this desktop.',
          ),
        ),
        findsNothing,
      );

      // Add a mouse keybind: sentence appears
      await _addKeybind(tester);
      final gesture = await tester.createGesture(
        kind: PointerDeviceKind.mouse,
        buttons: kBackMouseButton,
      );
      await gesture.down(const Offset(100, 100));
      await tester.pump();
      await gesture.up();
      await gesture.removePointer();
      await _pumpFor(tester, const Duration(milliseconds: 100));

      expect(
        _inPage(
          find.text(
            'Keybinds work in other apps too, through the portal. '
            'Mouse buttons work only while Opencord is focused on this desktop.',
          ),
        ),
        findsOneWidget,
      );

      // If global supports mouse (mouse: true), the sentence is absent even with mouse keybind
      app.repository.hotkeySupport = const HotkeySupport.global(
        'XInput',
        mouse: true,
      );
      app
          .read(hotkeySupportProvider.notifier)
          .set(const HotkeySupport.global('XInput', mouse: true));
      await tester.pump();

      expect(
        _inPage(find.text('Keybinds work in other apps too, through XInput.')),
        findsOneWidget,
      );
      expect(
        _inPage(
          find.textContaining(
            'Mouse buttons work only while Opencord is focused on this desktop.',
          ),
        ),
        findsNothing,
      );

      await app.dispose(tester);
    },
  );
}
