import 'dart:math' as math;
import 'dart:ui';

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// Largest gap between the red, green and blue channels (0–255) that still
/// counts as "no hue". The §2.1 greys carry a slight cool tint of up to 8
/// (for example `#6E6E76`); anything with real color is far above that.
const int maxChannelSpread = 8;

int channel(double value) => (value * 255).round();

int channelSpread(Color color) {
  final channels = [channel(color.r), channel(color.g), channel(color.b)];
  return channels.reduce(math.max) - channels.reduce(math.min);
}

bool isNeutral(Color color) => channelSpread(color) <= maxChannelSpread;

/// WCAG 2 contrast ratio of two opaque colors.
double contrastRatio(Color a, Color b) {
  final la = a.computeLuminance();
  final lb = b.computeLuminance();
  return (math.max(la, lb) + 0.05) / (math.min(la, lb) + 0.05);
}

/// Pixels of the image under [boundary] whose channels differ by more than
/// [maxChannelSpread], as "x,y #rrggbb" strings (at most [limit]).
Future<List<String>> huedPixels(
  WidgetTester tester,
  GlobalKey boundary, {
  int limit = 12,
}) async {
  final render =
      boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  final image = (await tester.runAsync(() => render.toImage()))!;
  final bytes = (await tester.runAsync(
    () => image.toByteData(format: ImageByteFormat.rawStraightRgba),
  ))!;
  final offenders = <String>[];
  for (var y = 0; y < image.height && offenders.length < limit; y++) {
    for (var x = 0; x < image.width && offenders.length < limit; x++) {
      final i = (y * image.width + x) * 4;
      final r = bytes.getUint8(i);
      final g = bytes.getUint8(i + 1);
      final b = bytes.getUint8(i + 2);
      final a = bytes.getUint8(i + 3);
      final spread = math.max(r, math.max(g, b)) - math.min(r, math.min(g, b));
      if (a > 0 && spread > maxChannelSpread) {
        final hex = [r, g, b].map((c) => c.toRadixString(16).padLeft(2, '0'));
        offenders.add('$x,$y #${hex.join()}');
      }
    }
  }
  image.dispose();
  return offenders;
}
