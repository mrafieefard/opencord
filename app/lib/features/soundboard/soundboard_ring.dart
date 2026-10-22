import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';

/// The sound [userId] is playing on [serverKey] now, if any (§11.6).
SoundPlay? playingSound(WidgetRef ref, String serverKey, int userId) =>
    ref.watch(soundPlaysProvider(serverKey).select((plays) => plays[userId]));

/// A dashed 2 px ring around [child] while a sound plays (Phase 2 plan
/// §11.6), slowly turning unless motion is reduced. Speaking keeps its
/// solid ring inside it. A circle around avatars; around tiles a rounded
/// rectangle of [radius].
class SoundboardRing extends StatefulWidget {
  const SoundboardRing({
    super.key,
    required this.playing,
    required this.child,
    this.radius,
    this.gap = 2,
  });

  final bool playing;
  final Widget child;

  /// A rounded rectangle with this radius; a circle when null.
  final double? radius;

  /// Between [child]'s edge and the ring.
  final double gap;

  @override
  State<SoundboardRing> createState() => _SoundboardRingState();
}

class _SoundboardRingState extends State<SoundboardRing>
    with SingleTickerProviderStateMixin {
  late final _turn = AnimationController(
    vsync: this,
    duration: const Duration(seconds: 8),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _follow();
  }

  @override
  void didUpdateWidget(SoundboardRing old) {
    super.didUpdateWidget(old);
    _follow();
  }

  /// Turns only while shown and while motion is allowed.
  void _follow() {
    final still = MediaQuery.disableAnimationsOf(context);
    if (widget.playing && !still) {
      if (!_turn.isAnimating) _turn.repeat();
    } else {
      _turn.stop();
    }
  }

  @override
  void dispose() {
    _turn.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!widget.playing) return widget.child;
    final color = context.oc.soundboardRing;
    return CustomPaint(
      foregroundPainter: _DashedRing(
        turn: _turn,
        color: color,
        radius: widget.radius,
        gap: widget.gap,
      ),
      child: widget.child,
    );
  }
}

class _DashedRing extends CustomPainter {
  _DashedRing({
    required this.turn,
    required this.color,
    required this.radius,
    required this.gap,
  }) : super(repaint: turn);

  final Animation<double> turn;
  final Color color;
  final double? radius;
  final double gap;

  static const _width = 2.0;
  static const _dash = 4.0;
  static const _space = 3.0;

  @override
  void paint(Canvas canvas, Size size) {
    final outset = gap + _width / 2;
    final rect = (Offset.zero & size).inflate(outset);
    final path = switch (radius) {
      final radius? =>
        Path()..addRRect(
          RRect.fromRectAndRadius(rect, Radius.circular(radius + outset)),
        ),
      null => Path()..addOval(rect),
    };
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = _width;
    const period = _dash + _space;
    for (final metric in path.computeMetrics()) {
      // Moving the dashes along the outline is the slow turn.
      var at = turn.value * metric.length % period - period;
      while (at < metric.length) {
        final start = math.max(at, 0.0);
        final end = math.min(at + _dash, metric.length);
        if (end > start) {
          canvas.drawPath(metric.extractPath(start, end), paint);
        }
        at += period;
      }
    }
  }

  @override
  bool shouldRepaint(_DashedRing old) =>
      old.color != color || old.radius != radius || old.gap != gap;
}
