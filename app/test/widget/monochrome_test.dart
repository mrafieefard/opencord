import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/ui/gallery/widget_gallery.dart';

import '../support/app.dart';
import '../support/colors.dart';
import '../support/pump.dart';

/// §13: no screen contains any hue, in either theme.
void main() {
  testWidgets('the scan notices real color', (tester) async {
    final boundary = GlobalKey();
    await pumpThemed(
      tester,
      RepaintBoundary(
        key: boundary,
        child: const ColoredBox(
          color: Color(0xFF14B8A6),
          child: SizedBox(width: 20, height: 20),
        ),
      ),
    );

    expect(await huedPixels(tester, boundary), isNotEmpty);
  });

  testWidgets('emoji are left out of the scan, and nothing else', (
    tester,
  ) async {
    Future<List<String>> scan(List<InlineSpan> spans) async {
      final boundary = GlobalKey();
      await pumpThemed(
        tester,
        RepaintBoundary(
          key: boundary,
          child: Text.rich(
            TextSpan(
              style: const TextStyle(fontSize: 20, color: Color(0xFFE11D48)),
              children: spans,
            ),
          ),
        ),
      );
      return huedPixels(tester, boundary);
    }

    expect(await scan([const TextSpan(text: '🎉👍')]), isEmpty);
    expect(await scan([const TextSpan(text: '🎉 red')]), isNotEmpty);
  });

  for (final (name, colors) in themes) {
    testWidgets('the widget gallery is monochrome in $name', (tester) async {
      final boundary = GlobalKey();
      await pumpThemed(
        tester,
        colors: colors,
        surface: const Size(720, 2600),
        RepaintBoundary(
          key: boundary,
          child: const SingleChildScrollView(
            child: GalleryContent(scrollable: false),
          ),
        ),
      );

      expect(await huedPixels(tester, boundary), isEmpty);
    });
  }

  for (final (name, theme) in const [
    ('dark', ThemePreference.dark),
    ('light', ThemePreference.light),
  ]) {
    testWidgets('the main screen is monochrome in $name', (tester) async {
      final app = await MockApp.pump(
        tester,
        settings: (settings) => settings.copyWith(theme: theme),
      );

      expect(await huedPixels(tester, MockApp.boundaryKey), isEmpty);
      await app.dispose(tester);
    });
  }
}
