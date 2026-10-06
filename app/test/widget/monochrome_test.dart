import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/gallery/widget_gallery.dart';

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
}
