import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/desktop/tray_icon.dart';

import '../support/pump.dart';

class _Mark extends CustomPainter {
  _Mark({required this.dark, required this.dot});

  final bool dark;
  final bool dot;

  @override
  void paint(Canvas canvas, Size size) =>
      paintTrayMark(canvas, size.width, trayMarkColor(dark: dark), dot: dot);

  @override
  bool shouldRepaint(_Mark oldDelegate) => false;
}

void main() {
  for (final (name, colors) in themes) {
    testWidgets('tray marks ($name)', (tester) async {
      final dark = name == 'dark';
      await pumpThemed(
        tester,
        colors: colors,
        background: (c) => c.rail,
        surface: const Size(4 * 56 + 16, 64),
        Padding(
          padding: const EdgeInsets.all(8),
          child: Row(
            children: [
              for (final size in [48.0, 22.0])
                for (final dot in [false, true])
                  Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: SizedBox.square(
                      dimension: 48,
                      child: Center(
                        child: CustomPaint(
                          size: Size.square(size),
                          painter: _Mark(dark: dark, dot: dot),
                        ),
                      ),
                    ),
                  ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/tray_marks_$name.png'),
      );
    });
  }

  test('pixels are reordered from RGBA to ARGB', () {
    final argb = rgbaToArgb(Uint8List.fromList([1, 2, 3, 4, 10, 20, 30, 40]));

    expect(argb, [4, 1, 2, 3, 40, 10, 20, 30]);
  });

  testWidgets('the icon renders to pixmaps of its size', (tester) async {
    final pixels = await tester.runAsync(
      () => renderTrayIcon(22, dark: true, dot: true),
    );

    expect(pixels, hasLength(22 * 22 * 4));
    expect(pixels!.any((value) => value != 0), isTrue);
  });
}
