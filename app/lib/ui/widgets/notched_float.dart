import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

/// The width of the page's border line.
const double _line = 1;

/// The main page's outline (the floating UI's design, §5.9): a rounded
/// rectangle of [size] with a [notch] square cut from its top right corner.
///
/// The outline runs around the notch with three joints of radius
/// `min(radius, notch / 2)`: convex where it meets the top edge, concave
/// inside, convex where it meets the right edge. It is intersected with the
/// plain rounded rectangle, so as [notch] reaches 0 the shape is exactly the
/// plain float's.
///
/// [inset] moves the outer edges in and the notch's edges out into the page,
/// keeping every corner's centre: a 1 px stroke at `inset: 0.5` lies inside
/// the shape, as a float's border does.
Path notchPath(
  Size size, {
  required double radius,
  required double notch,
  double inset = 0,
}) {
  final top = inset;
  final right = size.width - inset;
  final bottom = size.height - inset;
  final plain = Path()
    ..addRRect(
      RRect.fromLTRBR(
        inset,
        top,
        right,
        bottom,
        Radius.circular(math.max(radius - inset, 0)),
      ),
    );
  if (notch <= 0) return plain;

  final joint = math.min(radius, notch / 2);
  final convex = math.max(joint - inset, 0.0);
  // Below `notch / 2 == inset` the concave joint would run past the convex
  // ones; the notch then lies wholly in the plain corner's curve, which the
  // intersection keeps.
  final concave = math.min(joint + inset, notch - convex);
  final notchLeft = size.width - notch - inset;
  final notchBottom = notch + inset;
  final notched = Path()
    ..moveTo(inset, top)
    ..lineTo(notchLeft - convex, top)
    ..arcToPoint(
      Offset(notchLeft, top + convex),
      radius: Radius.circular(convex),
    )
    ..lineTo(notchLeft, notchBottom - concave)
    ..arcToPoint(
      Offset(notchLeft + concave, notchBottom),
      radius: Radius.circular(concave),
      clockwise: false,
    )
    ..lineTo(right - convex, notchBottom)
    ..arcToPoint(
      Offset(right, notchBottom + convex),
      radius: Radius.circular(convex),
    )
    ..lineTo(right, bottom)
    ..lineTo(inset, bottom)
    ..close();
  return Path.combine(PathOperation.intersect, notched, plain);
}

/// The main page's float (§5.9): a float whose top right corner gives way
/// to a [notch] square while the side slot is shut. One outline is used for
/// the background, the shadow, the border and the clip, so the shadow
/// follows the notch.
///
/// [notch] changes repaint and reclip without a rebuild, so it can follow an
/// animation. The notch is open space: its taps reach what lies behind.
class NotchedFloat extends StatelessWidget {
  const NotchedFloat({
    super.key,
    required this.notch,
    required this.child,
    this.color,
  });

  /// The notch's side, from 0 (shut) to [OcFloats.notch].
  final ValueListenable<double> notch;

  /// Sits inside the border, clipped to the outline's inner edge.
  final Widget child;

  /// The page's surface; `chat` when null.
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return ClipPath(
      // Takes hits only inside the outline. Nothing is clipped: the shadow
      // paints outside it.
      clipper: _NotchClipper(notch, line: 0),
      clipBehavior: Clip.none,
      child: CustomPaint(
        painter: _NotchPainter(
          notch,
          fill: color ?? colors.chat,
          border: colors.border,
          shadows: OcShadows.float(colors),
        ),
        child: Padding(
          padding: const EdgeInsets.all(_line),
          child: ClipPath(
            clipper: _NotchClipper(notch, line: _line),
            child: child,
          ),
        ),
      ),
    );
  }
}

/// The last outline built, kept while the size and the notch stay the same.
class _CachedPath {
  _CachedPath(this._build);

  final Path Function(Size size, double notch) _build;
  Size? _size;
  double? _notch;
  Path? _path;

  Path of(Size size, double notch) {
    final cached = _path;
    if (cached != null && size == _size && notch == _notch) return cached;
    _size = size;
    _notch = notch;
    return _path = _build(size, notch);
  }
}

class _NotchPainter extends CustomPainter {
  _NotchPainter(
    this.notch, {
    required this.fill,
    required this.border,
    required this.shadows,
  }) : super(repaint: notch);

  final ValueListenable<double> notch;
  final Color fill;
  final Color border;
  final List<BoxShadow> shadows;

  final _fill = _CachedPath(
    (size, notch) => notchPath(size, radius: OcFloats.radius, notch: notch),
  );
  final _stroke = _CachedPath(
    (size, notch) => notchPath(
      size,
      radius: OcFloats.radius,
      notch: notch,
      inset: _line / 2,
    ),
  );

  @override
  void paint(Canvas canvas, Size size) {
    final outline = _fill.of(size, notch.value);
    for (final shadow in shadows) {
      canvas
        ..save()
        ..translate(shadow.offset.dx, shadow.offset.dy)
        ..drawPath(outline, shadow.toPaint())
        ..restore();
    }
    canvas
      ..drawPath(outline, Paint()..color = fill)
      ..drawPath(
        _stroke.of(size, notch.value),
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = _line
          ..color = border,
      );
  }

  @override
  bool shouldRepaint(_NotchPainter old) =>
      old.notch != notch ||
      old.fill != fill ||
      old.border != border ||
      !listEquals(old.shadows, shadows);
}

/// The outline [line] in, for a box that sits [line] inside the float: 0 for
/// the float itself, the border's width for its content.
class _NotchClipper extends CustomClipper<Path> {
  _NotchClipper(this.notch, {required this.line})
    : _clip = _CachedPath(
        (size, notch) => notchPath(
          size + Offset(2 * line, 2 * line),
          radius: OcFloats.radius,
          notch: notch,
          inset: line,
        ).shift(Offset(-line, -line)),
      ),
      super(reclip: notch);

  final ValueListenable<double> notch;
  final double line;
  final _CachedPath _clip;

  @override
  Path getClip(Size size) => _clip.of(size, notch.value);

  @override
  bool shouldReclip(_NotchClipper old) =>
      old.notch != notch || old.line != line;
}
