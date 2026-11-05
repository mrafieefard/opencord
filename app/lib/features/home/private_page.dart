import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_motion.dart';

/// What a Friends or Requests page shows under its header (§5.16): padding
/// 12, a column at most 760 wide and centred, scrolling with its padding.
///
/// It fades in once, as the page opens. Switching tabs doesn't animate (§4.1).
class PrivatePage extends StatelessWidget {
  const PrivatePage({super.key, required this.rows, this.scrollKey});

  /// The rows for a column of the given width.
  final List<Widget> Function(double width) rows;

  /// Gives each tab a scroll position of its own.
  final Key? scrollKey;

  static const _padding = 12.0;
  static const _column = 760.0;

  @override
  Widget build(BuildContext context) {
    return TweenAnimationBuilder<double>(
      tween: Tween(begin: 0, end: 1),
      duration: OcMotion.of(context).fadeQuick,
      curve: OcMotion.curve,
      builder: (context, opacity, child) =>
          Opacity(opacity: opacity, child: child),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final side = math.max(_padding, (constraints.maxWidth - _column) / 2);
          return ListView(
            key: scrollKey,
            padding: EdgeInsets.symmetric(horizontal: side, vertical: _padding),
            children: rows(constraints.maxWidth - 2 * side),
          );
        },
      ),
    );
  }
}
