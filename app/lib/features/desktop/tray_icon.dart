import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';

import 'package:opencord/ui/theme/oc_colors.dart';

/// The sizes offered to tray hosts, which pick the closest.
const trayIconSizes = [16, 22, 24, 32, 48];

/// The monochrome tray mark (§15): a speech bubble ring, with a dot cut
/// into its top right while something is unread. Opencord has no logo yet;
/// this stands in for one.
void paintTrayMark(
  Canvas canvas,
  double size,
  Color color, {
  bool dot = false,
}) {
  final paint = Paint()
    ..color = color
    ..isAntiAlias = true;
  canvas.saveLayer(Offset.zero & Size.square(size), Paint());
  canvas.drawCircle(
    Offset(size * 0.5, size * 0.47),
    size * 0.32,
    Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = size * 0.13,
  );
  canvas.drawPath(
    Path()
      ..moveTo(size * 0.16, size * 0.9)
      ..lineTo(size * 0.22, size * 0.6)
      ..lineTo(size * 0.42, size * 0.76)
      ..close(),
    paint,
  );
  if (dot) {
    final center = Offset(size * 0.8, size * 0.2);
    canvas.drawCircle(
      center,
      size * 0.27,
      Paint()..blendMode = BlendMode.clear,
    );
    canvas.drawCircle(center, size * 0.17, paint);
  }
  canvas.restore();
}

/// The mark's color: white on a dark desktop, black on a light one.
Color trayMarkColor({required bool dark}) =>
    (dark ? OcColors.dark : OcColors.light).text;

/// The mark at [size] as ARGB32 in network byte order, the
/// StatusNotifierItem pixmap format.
Future<Uint8List> renderTrayIcon(
  int size, {
  required bool dark,
  required bool dot,
}) async {
  final recorder = ui.PictureRecorder();
  paintTrayMark(
    Canvas(recorder),
    size.toDouble(),
    trayMarkColor(dark: dark),
    dot: dot,
  );
  final image = await recorder.endRecording().toImage(size, size);
  final data = await image.toByteData(
    format: ui.ImageByteFormat.rawStraightRgba,
  );
  image.dispose();
  return rgbaToArgb(data!.buffer.asUint8List());
}

/// Reorders RGBA pixels to ARGB.
Uint8List rgbaToArgb(Uint8List rgba) {
  final argb = Uint8List(rgba.length);
  for (var i = 0; i + 3 < rgba.length; i += 4) {
    argb[i] = rgba[i + 3];
    argb[i + 1] = rgba[i];
    argb[i + 2] = rgba[i + 1];
    argb[i + 3] = rgba[i + 2];
  }
  return argb;
}
