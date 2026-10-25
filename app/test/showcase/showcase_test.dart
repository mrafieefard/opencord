// Screenshots of the app on the mock world, for the README and for showing
// it off. Skipped unless OPENCORD_SHOWCASE names a folder to write them to:
//
//   OPENCORD_SHOWCASE=$PWD/../docs/images flutter test test/showcase
import 'dart:io';
import 'dart:ui' show ImageByteFormat;

import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../support/app.dart';

final _out = Platform.environment['OPENCORD_SHOWCASE'];

/// A color emoji font to draw emoji with, as the system would: tests have
/// no system fonts. Noto Color Emoji (OFL) works.
final _emoji = Platform.environment['OPENCORD_SHOWCASE_EMOJI'];
const _size = Size(1600, 1000);

Future<void> _settle(WidgetTester tester, [int ms = 600]) async {
  for (var waited = 0; waited < ms; waited += 50) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

/// Gives pictures time to decode, which happens outside the test's clock.
Future<void> _decodeImages(WidgetTester tester) async {
  for (var i = 0; i < 5; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 100)),
    );
    await tester.pump(const Duration(milliseconds: 50));
  }
}

/// Renders the whole window at twice its size into `<name>.png`.
Future<void> _shoot(WidgetTester tester, String name) async {
  final boundary =
      MockApp.boundaryKey.currentContext!.findRenderObject()!
          as RenderRepaintBoundary;
  final image = (await tester.runAsync(() => boundary.toImage(pixelRatio: 2)))!;
  final bytes = (await tester.runAsync(
    () => image.toByteData(format: ImageByteFormat.png),
  ))!;
  File('$_out/$name.png').writeAsBytesSync(bytes.buffer.asUint8List());
}

Finder _inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

void main() {
  setUpAll(() async {
    if (_emoji case final emoji?) {
      final loader = FontLoader('Noto Color Emoji')
        ..addFont(File(emoji).readAsBytes().then(ByteData.sublistView));
      await loader.load();
    }
  });

  testWidgets('showcase', skip: _out == null, (tester) async {
    final app = await MockApp.pump(
      tester,
      size: _size,
      platform: TargetPlatform.linux,
      settings: (s) => s.copyWith(theme: ThemePreference.dark),
    );
    await _settle(tester);

    await tester.tap(_inSidebar('general'));
    await _settle(tester);
    await _shoot(tester, 'chat');

    await tester.tap(_inSidebar('design-review'));
    await _settle(tester);
    await _decodeImages(tester);
    await _shoot(tester, 'chat-media');

    await tester.tap(_inSidebar('General'));
    await _settle(tester, 1500);
    await _shoot(tester, 'voice');

    await tester.tap(find.widgetWithText(OcButton, 'Watch'));
    await _settle(tester, 1500);
    await _shoot(tester, 'voice-watch');

    await tester.tap(find.text('Private').first);
    await _settle(tester);
    await tester.tap(find.text('Mira Okafor').first);
    await _settle(tester);
    await _decodeImages(tester);
    await _shoot(tester, 'private');

    await tester.tap(
      find.byWidgetPredicate(
        (widget) => widget is OcIconButton && widget.tooltip == 'User settings',
      ),
    );
    await _settle(tester);
    await tester.tap(find.text('Appearance'));
    await _settle(tester);
    await _shoot(tester, 'settings');
    await app.dispose(tester);
  });

  testWidgets('showcase in the light theme', skip: _out == null, (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      size: _size,
      platform: TargetPlatform.linux,
      settings: (s) => s.copyWith(theme: ThemePreference.light),
    );
    await _settle(tester);
    await tester.tap(_inSidebar('general'));
    await _settle(tester);
    await _shoot(tester, 'chat-light');
    await app.dispose(tester);
  });
}
