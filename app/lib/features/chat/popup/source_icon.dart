import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/chat/popup/popup_rows.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';

/// Where a command comes from, as a small rounded square: the Opencord
/// mark, a plugin's icon, or its initials when it has none.
class CommandSourceIcon extends ConsumerWidget {
  const CommandSourceIcon({
    super.key,
    required this.serverKey,
    required this.source,
    required this.size,
    required this.radius,
    this.color,
    this.background,
  });

  final String serverKey;
  final PopupSource source;
  final double size;
  final double radius;

  /// The mark's and initials' colour; the text colour by default.
  final Color? color;

  /// The square's colour; the first avatar shade by default.
  final Color? background;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final foreground = color ?? colors.text;
    final image = source.avatarHash.isEmpty
        ? null
        : avatarImage(ref, serverKey, source.avatarHash);
    return Container(
      width: size,
      height: size,
      clipBehavior: Clip.antiAlias,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: background ?? colors.avatarShades.first,
        borderRadius: BorderRadius.circular(radius),
      ),
      child: switch (image) {
        final image? => Image(
          image: image,
          width: size,
          height: size,
          fit: BoxFit.cover,
        ),
        null when source.isOpencord => CustomPaint(
          size: Size.square(size * 0.62),
          painter: _OpencordMark(foreground),
        ),
        null => Text(
          _initials(source.name),
          maxLines: 1,
          overflow: TextOverflow.clip,
          style: TextStyle(
            color: foreground,
            fontSize: size * 0.34,
            fontWeight: FontWeight.w600,
            height: 1,
          ),
        ),
      },
    );
  }
}

String _initials(String name) => name
    .split(RegExp(r'\s+'))
    .where((word) => word.isNotEmpty)
    .take(2)
    .map((word) => word.characters.first.toUpperCase())
    .join();

/// A circle with a slash through it, drawn on a 24-unit grid.
class _OpencordMark extends CustomPainter {
  const _OpencordMark(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final unit = size.width / 24;
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.8 * unit
      ..strokeCap = StrokeCap.round;
    canvas
      ..drawCircle(Offset(12 * unit, 12 * unit), 8.5 * unit, paint)
      ..drawLine(
        Offset(14.5 * unit, 7.5 * unit),
        Offset(9.5 * unit, 16.5 * unit),
        paint,
      );
  }

  @override
  bool shouldRepaint(_OpencordMark old) => old.color != color;
}
