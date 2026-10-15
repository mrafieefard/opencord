import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// The signal bars' icon and how they read (Phase 2 §12), for
/// [VoiceStats.bars].
(IconData, String) connectionQuality(int bars) => switch (bars) {
  3 => (OcIcons.signalCellularAlt, 'good'),
  2 => (OcIcons.signalCellularAlt2Bar, 'fair'),
  _ => (OcIcons.signalCellularAlt1Bar, 'poor'),
};

/// The voice connection's last minute (Phase 2 §12): ping, packet loss and
/// jitter, each a line with its latest value, as the reports come.
class ConnectionGraph extends ConsumerWidget {
  const ConnectionGraph({super.key});

  static const width = 280.0;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final history = ref.watch(voiceStatsHistoryProvider);
    if (history.isEmpty) {
      return SizedBox(
        width: width,
        child: Text(
          'Waiting for the first measurement.',
          style: OcText.small.copyWith(color: colors.textSecondary),
        ),
      );
    }
    String ms(double? value) =>
        value == null ? '–' : '${value.toStringAsFixed(0)} ms';
    return SizedBox(
      width: width,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _Chart(
            label: 'Ping',
            values: [for (final s in history) s.pingMs?.toDouble()],
            shown: ms,
            floor: 100,
          ),
          const SizedBox(height: OcSpace.s12),
          _Chart(
            label: 'Packet loss',
            values: [for (final s in history) s.lossPercent],
            shown: (value) =>
                value == null ? '–' : '${value.toStringAsFixed(1)} %',
            floor: 5,
          ),
          const SizedBox(height: OcSpace.s12),
          _Chart(
            label: 'Jitter',
            values: [for (final s in history) s.jitterMs],
            shown: ms,
            floor: 20,
          ),
        ],
      ),
    );
  }
}

/// One measurement over the last minute, newest at the right, with its
/// latest value. [floor] is the least the chart's top stands for, so a
/// quiet line stays low instead of filling the chart.
class _Chart extends StatelessWidget {
  const _Chart({
    required this.label,
    required this.values,
    required this.shown,
    required this.floor,
  });

  final String label;
  final List<double?> values;
  final String Function(double?) shown;
  final double floor;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final latest = shown(values.last);
    return Semantics(
      label: '$label, $latest',
      excludeSemantics: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  label,
                  style: OcText.small.copyWith(color: colors.textSecondary),
                ),
              ),
              Text(
                latest,
                style: OcText.small.copyWith(
                  color: colors.text,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          ),
          const SizedBox(height: OcSpace.s4),
          SizedBox(
            height: 40,
            child: CustomPaint(
              painter: _LinePainter(
                values: values,
                top: math.max(
                  floor,
                  values.whereType<double>().fold(0.0, math.max) * 1.25,
                ),
                line: colors.text,
                grid: colors.border,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _LinePainter extends CustomPainter {
  const _LinePainter({
    required this.values,
    required this.top,
    required this.line,
    required this.grid,
  });

  final List<double?> values;
  final double top;
  final Color line;
  final Color grid;

  @override
  void paint(Canvas canvas, Size size) {
    final gridPaint = Paint()
      ..color = grid
      ..strokeWidth = 1;
    for (final y in [0.5, size.height / 2, size.height - 0.5]) {
      canvas.drawLine(Offset(0, y), Offset(size.width, y), gridPaint);
    }
    // A minute across, newest at the right edge; a missing value breaks
    // the line.
    final step = size.width / (VoiceStatsHistoryNotifier.length - 1);
    final start = VoiceStatsHistoryNotifier.length - values.length;
    final stroke = Path();
    final fill = Path();
    Offset? previous;
    Offset? runStart;
    void closeRun() {
      if (previous case final end? when runStart != null) {
        fill
          ..lineTo(end.dx, size.height)
          ..lineTo(runStart!.dx, size.height)
          ..close();
      }
      runStart = null;
    }

    for (var i = 0; i < values.length; i++) {
      final value = values[i];
      if (value == null) {
        closeRun();
        previous = null;
        continue;
      }
      final point = Offset(
        (start + i) * step,
        size.height - (value / top).clamp(0.0, 1.0) * (size.height - 1),
      );
      if (previous == null) {
        stroke.moveTo(point.dx, point.dy);
        fill.moveTo(point.dx, point.dy);
        runStart = point;
      } else {
        stroke.lineTo(point.dx, point.dy);
        fill.lineTo(point.dx, point.dy);
      }
      previous = point;
    }
    closeRun();
    canvas
      ..drawPath(fill, Paint()..color = line.withValues(alpha: 0.08))
      ..drawPath(
        stroke,
        Paint()
          ..color = line
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5
          ..strokeJoin = StrokeJoin.round,
      );
    // A lone report is a dot.
    if (values.length == 1 && previous != null) {
      canvas.drawCircle(previous, 1.5, Paint()..color = line);
    }
  }

  @override
  bool shouldRepaint(_LinePainter old) =>
      old.values != values ||
      old.top != top ||
      old.line != line ||
      old.grid != grid;
}
