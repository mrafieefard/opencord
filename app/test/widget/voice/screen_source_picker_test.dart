import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/voice/screen_source_picker.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';

// Opencord's own source picker (docs/plans/windows.md W6, step 8), on the
// mock repository's fake sources. Windows is the system that has it:
// `ownScreenPicker` is true there, so the share dialog lists the screens
// and windows itself instead of leaving it to the desktop. Everywhere else
// `RepoCapabilities.everything` leaves it false, which is why the rest of
// the voice tests see the dialog they always did.

/// The app as Windows has it: Opencord picks the source.
Future<MockApp> _windows(WidgetTester tester) => MockApp.pump(
  tester,
  platform: TargetPlatform.windows,
  capabilities: const RepoCapabilities(
    reactions: true,
    pins: true,
    voice: true,
    camera: true,
    screenShare: true,
    announcementChannels: true,
    readStates: true,
    replies: true,
    soundboard: true,
    ownScreenPicker: true,
  ),
);

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _joinGeneral(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('General'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

Future<void> _openShare(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byType(VoiceControlBar),
      matching: find.bySemanticsLabel('Share your screen'),
    ),
  );
  // Long enough for the first list of sources to come back.
  await _pumpFor(tester, const Duration(milliseconds: 600));
}

OcButton _goLive(WidgetTester tester) =>
    tester.widget<OcButton>(find.widgetWithText(OcButton, 'Go live'));

void main() {
  testWidgets('the share dialog lists the screens and windows', (tester) async {
    final app = await _windows(tester);
    await _joinGeneral(tester);
    await _openShare(tester);

    expect(find.byType(ScreenSourcePicker), findsOneWidget);
    expect(find.text('Screens (1)'), findsOneWidget);
    expect(find.text('Windows (1)'), findsOneWidget);
    expect(find.text('Screen 1 · 2560×1440'), findsOneWidget);

    // The windows are behind their own tab, with the app under the title.
    await tester.tap(find.text('Windows (1)'));
    await _pumpFor(tester, const Duration(milliseconds: 400));
    expect(find.text('Notes — Opencord plan'), findsOneWidget);
    expect(find.text('Notes'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('Go live waits for a source, and sends the one chosen', (
    tester,
  ) async {
    final app = await _windows(tester);
    await _joinGeneral(tester);
    await _openShare(tester);

    // Nothing chosen yet, so there is nothing to go live with.
    expect(
      _goLive(tester).onPressed,
      isNull,
      reason: 'Go live waits until a screen or window is chosen',
    );

    await tester.tap(find.text('Screen 1 · 2560×1440'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(_goLive(tester).onPressed, isNotNull);

    await tester.tap(find.widgetWithText(OcButton, 'Go live'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(
      app.repository.lastScreenSourceId,
      'screen:1',
      reason: 'the chosen id reaches the repository',
    );
    expect(app.read(voiceSessionProvider).screensharing, isTrue);
    await app.dispose(tester);
  });

  testWidgets('a source with a picture draws it, one without still shows', (
    tester,
  ) async {
    final app = await _windows(tester);
    app.repository.debugScreenSources = [
      (
        id: 'screen:1',
        kind: StreamSource.screen,
        title: 'Screen 1 · 1920×1080',
        app: null,
        // Four red pixels, as the core packs them: RGBA rows.
        thumbnail: (
          width: 2,
          height: 2,
          rgba: Uint8List.fromList([
            for (var pixel = 0; pixel < 4; pixel++) ...[255, 0, 0, 255],
          ]),
        ),
      ),
      (
        id: 'screen:2',
        kind: StreamSource.screen,
        title: 'Screen 2 · 1280×720',
        app: null,
        thumbnail: null,
      ),
    ];
    await _joinGeneral(tester);
    await _openShare(tester);

    expect(find.text('Screen 1 · 1920×1080'), findsOneWidget);
    expect(find.text('Screen 2 · 1280×720'), findsOneWidget);
    // The one with pixels draws them; the one without falls back to a
    // plain block rather than to an error.
    expect(find.byType(Image), findsOneWidget);
    expect(find.byIcon(Icons.desktop_windows_outlined), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('nothing to share says so rather than looking broken', (
    tester,
  ) async {
    final app = await _windows(tester);
    app.repository.debugScreenSources = [];
    await _joinGeneral(tester);
    await _openShare(tester);

    expect(find.text('No screens were found.'), findsOneWidget);
    await tester.tap(find.text('Windows (0)'));
    await _pumpFor(tester, const Duration(milliseconds: 400));
    expect(find.text('No windows are open to share.'), findsOneWidget);
    expect(
      _goLive(tester).onPressed,
      isNull,
      reason: 'nothing to go live with',
    );
    await app.dispose(tester);
  });

  testWidgets('Change source asks again, and sends the new one', (
    tester,
  ) async {
    final app = await _windows(tester);
    await _joinGeneral(tester);
    await _openShare(tester);
    await tester.tap(find.text('Screen 1 · 2560×1440'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Go live'));
    await _pumpFor(tester, const Duration(milliseconds: 600));
    expect(app.repository.lastScreenSourceId, 'screen:1');

    // Live now: the button is the share's options rather than a way to
    // start one, and it opens a menu.
    await tester.tap(
      find.descendant(
        of: find.byType(VoiceControlBar),
        matching: find.bySemanticsLabel('Screen share options'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.text('Change source'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    // The picker again — and only the picker: the quality and the sound
    // are not being asked about.
    expect(find.byType(ScreenSourcePicker), findsOneWidget);
    expect(find.text('Resolution'), findsNothing);
    expect(find.text('Share sound'), findsNothing);

    await tester.tap(find.text('Windows (1)'));
    await _pumpFor(tester, const Duration(milliseconds: 400));
    await tester.tap(find.text('Notes — Opencord plan'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Share this'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(
      app.repository.lastScreenSourceId,
      'window:42',
      reason: 'the new source reaches the repository',
    );
    await app.dispose(tester);
  });
}
