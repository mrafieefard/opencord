import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';

import 'package:opencord/features/desktop/tray_menu.dart';
import 'package:opencord/ui/theme/oc_colors.dart';

/// The sizes offered to tray hosts, which pick the closest.
const trayIconSizes = [16, 22, 24, 32, 48];

/// The monochrome tray mark (§15). Out of voice: a speech bubble ring
/// (Opencord has no logo yet; this stands in for one). In voice, as
/// Discord does: a ring, filled while you talk, slashed while muted. A dot
/// is cut into the top right while something is unread.
void paintTrayMark(
  Canvas canvas,
  double size,
  Color color, {
  bool dot = false,
  TrayVoice voice = TrayVoice.none,
}) {
  final paint = Paint()
    ..color = color
    ..isAntiAlias = true;
  final stroke = Paint()
    ..color = color
    ..isAntiAlias = true
    ..style = PaintingStyle.stroke
    ..strokeWidth = size * 0.13;
  final center = Offset(size * 0.5, size * 0.5);
  canvas.saveLayer(Offset.zero & Size.square(size), Paint());
  switch (voice) {
    case TrayVoice.none:
      canvas.drawCircle(Offset(size * 0.5, size * 0.47), size * 0.32, stroke);
      canvas.drawPath(
        Path()
          ..moveTo(size * 0.16, size * 0.9)
          ..lineTo(size * 0.22, size * 0.6)
          ..lineTo(size * 0.42, size * 0.76)
          ..close(),
        paint,
      );
    case TrayVoice.quiet:
      canvas.drawCircle(center, size * 0.36, stroke);
    case TrayVoice.speaking:
      canvas.drawCircle(center, size * 0.42, paint);
    case TrayVoice.muted:
      canvas.drawCircle(center, size * 0.36, stroke);
      canvas.drawLine(
        Offset(size * 0.18, size * 0.18),
        Offset(size * 0.82, size * 0.82),
        stroke..strokeCap = StrokeCap.round,
      );
  }
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
  TrayVoice voice = TrayVoice.none,
}) async => rgbaToArgb(
  await renderTrayIconRgba(
    size,
    color: trayMarkColor(dark: dark),
    dot: dot,
    voice: voice,
  ),
);

/// The mark at [size] in [color], as straight (not premultiplied) RGBA.
Future<Uint8List> renderTrayIconRgba(
  int size, {
  required Color color,
  required bool dot,
  TrayVoice voice = TrayVoice.none,
}) async {
  final recorder = ui.PictureRecorder();
  paintTrayMark(
    Canvas(recorder),
    size.toDouble(),
    color,
    dot: dot,
    voice: voice,
  );
  final image = await recorder.endRecording().toImage(size, size);
  final data = await image.toByteData(
    format: ui.ImageByteFormat.rawStraightRgba,
  );
  image.dispose();
  return data!.buffer.asUint8List();
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
