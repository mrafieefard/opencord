import 'dart:math' as math;

import 'package:flutter/material.dart';

/// Resamples a waveform (e.g. 128 bytes) to [count] bars by max-pooling.
List<int> resampleWaveform(List<int> values, int count) {
  if (count <= 0 || values.isEmpty) return const [];
  final result = <int>[];
  for (var i = 0; i < count; i++) {
    final start = (i * values.length) ~/ count;
    var end = ((i + 1) * values.length) ~/ count;
    if (end <= start) end = start + 1;
    if (end > values.length) end = values.length;
    var maxVal = values[start];
    for (var j = start + 1; j < end; j++) {
      if (values[j] > maxVal) maxVal = values[j];
    }
    result.add(maxVal);
  }
  return result;
}

class WaveformPainter extends CustomPainter {
  WaveformPainter.live({required this.bars, required this.color})
    : isLive = true,
      staticValues = const [],
      progress = 0,
      playedColor = color,
      unplayedColor = color;

  WaveformPainter.static({
    required List<int> values,
    required this.progress,
    required this.playedColor,
    required this.unplayedColor,
  }) : isLive = false,
       staticValues = values,
       bars = const [],
       color = playedColor;

  final bool isLive;
  final List<double> bars;
  final Color color;

  final List<int> staticValues;
  final double progress;
  final Color playedColor;
  final Color unplayedColor;

  static const double barWidth = 3;
  static const double barGap = 2;
  static const double barSlot = barWidth + barGap;
  static const double minBarHeight = 3;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;
    final paint = Paint()..style = PaintingStyle.fill;
    final maxH = size.height;

    if (isLive) {
      paint.color = color;
      final maxBars = (size.width / barSlot).floor();
      if (maxBars <= 0 || bars.isEmpty) return;

      final visibleBars = bars.length > maxBars
          ? bars.sublist(bars.length - maxBars)
          : bars;

      for (var i = 0; i < visibleBars.length; i++) {
        final barIdx = visibleBars.length - 1 - i;
        final x = size.width - barWidth - i * barSlot;
        if (x < 0) break;

        final level = visibleBars[barIdx].clamp(0.0, 1.0);
        final h = math.max(minBarHeight, level * maxH);
        final y = (size.height - h) / 2;
        final rrect = RRect.fromRectAndRadius(
          Rect.fromLTWH(x, y, barWidth, h),
          const Radius.circular(1.5),
        );
        canvas.drawRRect(rrect, paint);
      }
    } else {
      final count = (size.width / barSlot).floor();
      if (count <= 0 || staticValues.isEmpty) return;

      final resampled = resampleWaveform(staticValues, count);
      for (var i = 0; i < resampled.length; i++) {
        final x = i * barSlot;
        final level = (resampled[i] / 255.0).clamp(0.0, 1.0);
        final h = math.max(minBarHeight, level * maxH);
        final y = (size.height - h) / 2;

        final isPlayed = progress > 0 && (i / count) < progress;
        paint.color = isPlayed ? playedColor : unplayedColor;

        final rrect = RRect.fromRectAndRadius(
          Rect.fromLTWH(x, y, barWidth, h),
          const Radius.circular(1.5),
        );
        canvas.drawRRect(rrect, paint);
      }
    }
  }

  @override
  bool shouldRepaint(covariant WaveformPainter oldDelegate) {
    if (isLive != oldDelegate.isLive) return true;
    if (isLive) {
      return bars != oldDelegate.bars || color != oldDelegate.color;
    }
    return staticValues != oldDelegate.staticValues ||
        progress != oldDelegate.progress ||
        playedColor != oldDelegate.playedColor ||
        unplayedColor != oldDelegate.unplayedColor;
  }
}
