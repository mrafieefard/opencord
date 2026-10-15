import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
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

/// The voice connection (Phase 2 §12): ping over the last five minutes,
/// then the voice node, the average and last ping, packet loss and jitter,
/// as the reports come.
class ConnectionGraph extends ConsumerWidget {
  const ConnectionGraph({super.key});

  static const width = 280.0;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final history = ref.watch(voiceStatsHistoryProvider);
    final title = Text(
      'Connection',
      style: OcText.bodyStrong.copyWith(color: colors.text),
    );
    if (history.isEmpty) {
      return SizedBox(
        width: width,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            title,
            const SizedBox(height: OcSpace.s8),
            Text(
              'Waiting for the first measurement.',
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
          ],
        ),
      );
    }
    final last = history.last.stats;
    final pings = [for (final entry in history) ?entry.stats.pingMs];
    final average = pings.isEmpty
        ? null
        : (pings.reduce((a, b) => a + b) / pings.length).round();
    String ms(num? value) => value == null ? '–' : '$value ms';
    final rows = <(String, String)>[
      ('Average ping', ms(average)),
      ('Last ping', ms(last.pingMs)),
      (
        'Packet loss rate',
        switch (last.lossPercent) {
          null => '–',
          final loss => '${loss.toStringAsFixed(1)}%',
        },
      ),
      ('Jitter', ms(last.jitterMs?.round())),
    ];
    return SizedBox(
      width: width,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          title,
          const SizedBox(height: OcSpace.s8),
          Divider(height: 1, color: colors.border),
          const SizedBox(height: OcSpace.s12),
          _PingChart(history: history),
          const SizedBox(height: OcSpace.s12),
          if (last.voiceNode.isNotEmpty) ...[
            Text(
              last.voiceNode,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: OcText.bodyStrong.copyWith(color: colors.text),
            ),
            const SizedBox(height: OcSpace.s6),
          ],
          for (final (label, value) in rows)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 1),
              child: Text.rich(
                TextSpan(
                  text: '$label: ',
                  children: [
                    TextSpan(
                      text: value,
                      style: const TextStyle(fontWeight: FontWeight.w700),
                    ),
                  ],
                ),
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ),
        ],
      ),
    );
  }
}

/// Ping over the last five minutes, newest at the right: a line on a dark
/// panel, the minutes along the bottom and the scale up the right.
class _PingChart extends StatelessWidget {
  const _PingChart({required this.history});

  final List<({DateTime at, VoiceStats stats})> history;

  static const _plotHeight = 72.0;
  static const _scaleWidth = 26.0;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final end = history.last.at;
    final start = end.subtract(VoiceStatsHistoryNotifier.span);
    final top = _niceTop(
      history.map((entry) => entry.stats.pingMs ?? 0).fold(0, math.max),
    );
    // Whole minutes inside the window, where the gridlines and labels go.
    final minutes = [
      for (
        var minute = DateTime(
          start.year,
          start.month,
          start.day,
          start.hour,
          start.minute + 1,
        );
        !minute.isAfter(end);
        minute = minute.add(const Duration(minutes: 1))
      )
        minute,
    ];
    double along(DateTime time, double width) =>
        width *
        time.difference(start).inMilliseconds /
        VoiceStatsHistoryNotifier.span.inMilliseconds;
    final label = OcText.meta.copyWith(color: colors.textMuted, fontSize: 10);
    return Semantics(
      label:
          'Ping over the last five minutes, '
          'last ${history.last.stats.pingMs ?? '–'} ms',
      excludeSemantics: true,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final width = constraints.maxWidth;
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Container(
                      height: _plotHeight,
                      decoration: BoxDecoration(
                        color: colors.rail,
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: CustomPaint(
                        painter: _PingPainter(
                          points: [
                            for (final entry in history)
                              (
                                x: along(entry.at, width),
                                ping: entry.stats.pingMs?.toDouble(),
                              ),
                          ],
                          gridlines: [
                            for (final minute in minutes) along(minute, width),
                          ],
                          top: top.toDouble(),
                          line: colors.text,
                          grid: colors.border,
                        ),
                      ),
                    ),
                    const SizedBox(height: 3),
                    SizedBox(
                      height: 14,
                      child: Stack(
                        clipBehavior: Clip.none,
                        children: [
                          for (final minute in minutes)
                            if (along(minute, width) case final x
                                when x > 14 && x < width - 14)
                              Positioned(
                                left: x - 20,
                                width: 40,
                                child: Text(
                                  clockTime(minute),
                                  textAlign: TextAlign.center,
                                  style: label,
                                ),
                              ),
                        ],
                      ),
                    ),
                  ],
                );
              },
            ),
          ),
          SizedBox(
            width: _scaleWidth,
            height: _plotHeight,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                for (final value in [top, top ~/ 2, 0])
                  Text('$value', style: label),
              ],
            ),
          ),
        ],
      ),
    );
  }

  /// The chart's top: a round number a little above the highest ping, and
  /// never under 10 ms.
  static int _niceTop(int highest) {
    final wanted = math.max(10, (highest * 1.2).ceil());
    var step = 1;
    while (true) {
      for (final nice in [1, 2, 5]) {
        if (nice * step >= wanted) return nice * step;
      }
      step *= 10;
    }
  }
}

class _PingPainter extends CustomPainter {
  const _PingPainter({
    required this.points,
    required this.gridlines,
    required this.top,
    required this.line,
    required this.grid,
  });

  final List<({double x, double? ping})> points;
  final List<double> gridlines;
  final double top;
  final Color line;
  final Color grid;

  /// Reports further apart than this were not one line: the call broke.
  static const _gap = 3.0;

  @override
  void paint(Canvas canvas, Size size) {
    final gridPaint = Paint()
      ..color = grid
      ..strokeWidth = 1;
    for (final x in gridlines) {
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), gridPaint);
    }
    final secondWidth = size.width / VoiceStatsHistoryNotifier.span.inSeconds;
    final path = Path();
    ({double x, double y})? previous;
    for (final point in points) {
      final ping = point.ping;
      if (ping == null) {
        previous = null;
        continue;
      }
      final y =
          size.height - 1 - (ping / top).clamp(0.0, 1.0) * (size.height - 2);
      if (previous == null || point.x - previous.x > _gap * secondWidth) {
        path.moveTo(point.x, y);
      } else {
        path.lineTo(point.x, y);
      }
      previous = (x: point.x, y: y);
    }
    canvas
      ..clipRect(Offset.zero & size)
      ..drawPath(
        path,
        Paint()
          ..color = line
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5
          ..strokeJoin = StrokeJoin.round,
      );
    if (points.length == 1 && previous != null) {
      canvas.drawCircle(
        Offset(previous.x, previous.y),
        1.5,
        Paint()..color = line,
      );
    }
  }

  @override
  bool shouldRepaint(_PingPainter old) =>
      old.points != points ||
      old.top != top ||
      old.line != line ||
      old.grid != grid;
}
