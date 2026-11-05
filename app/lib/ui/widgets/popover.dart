import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/widgets/popup_route.dart';

export 'package:opencord/ui/widgets/popup_route.dart' show globalRectOf;

enum PopoverSide { below, above, right, left }

/// Computes the corner nearest the trigger for a popover next to [anchor].
Alignment popoverAlignment({
  required Rect anchor,
  required PopoverSide side,
  required bool alignEnd,
  required Size mediaSize,
  double gap = 6,
}) {
  const estimatedSize = 100.0;
  final below = anchor.bottom + gap;
  final above = anchor.top - gap - estimatedSize;
  final right = anchor.right + gap;
  final left = anchor.left - gap - estimatedSize;

  final fitsBelow = below + estimatedSize <= mediaSize.height - popupMargin;
  final fitsAbove = above >= popupMargin;
  final fitsRight = right + estimatedSize <= mediaSize.width - popupMargin;
  final fitsLeft = left >= popupMargin;

  final actualSide = switch (side) {
    PopoverSide.below =>
      (fitsBelow || !fitsAbove) ? PopoverSide.below : PopoverSide.above,
    PopoverSide.above =>
      (fitsAbove || !fitsBelow) ? PopoverSide.above : PopoverSide.below,
    PopoverSide.right =>
      (fitsRight || !fitsLeft) ? PopoverSide.right : PopoverSide.left,
    PopoverSide.left =>
      (fitsLeft || !fitsRight) ? PopoverSide.left : PopoverSide.right,
  };

  return switch (actualSide) {
    PopoverSide.below => alignEnd ? Alignment.topRight : Alignment.topLeft,
    PopoverSide.above =>
      alignEnd ? Alignment.bottomRight : Alignment.bottomLeft,
    PopoverSide.right => alignEnd ? Alignment.bottomLeft : Alignment.topLeft,
    PopoverSide.left => alignEnd ? Alignment.bottomRight : Alignment.topRight,
  };
}

/// Opens [builder] next to [anchor] (a global rectangle, see
/// [globalRectOf]), on [side] when it fits and on the opposite side when it
/// does not.
Future<T?> showPopover<T>({
  required BuildContext context,
  required Rect anchor,
  required WidgetBuilder builder,
  PopoverSide side = PopoverSide.below,
  bool alignEnd = false,
  double gap = 6,
  EdgeInsets padding = const EdgeInsets.all(12),
}) {
  final media = MediaQuery.sizeOf(context);
  final alignment = popoverAlignment(
    anchor: anchor,
    side: side,
    alignEnd: alignEnd,
    mediaSize: media,
    gap: gap,
  );
  return pushPopup<T>(
    context,
    layout: _PopoverLayout(anchor, side, alignEnd, gap),
    alignment: alignment,
    builder: (context) => PopoverPanel(
      padding: padding,
      child: Builder(builder: builder),
    ),
  );
}

/// The elevated surface of a popover.
class PopoverPanel extends StatelessWidget {
  const PopoverPanel({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(12),
  });

  final Widget child;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    return Material(
      type: MaterialType.transparency,
      child: Container(
        padding: padding,
        decoration: popupDecoration(context.oc),
        child: child,
      ),
    );
  }
}

class _PopoverLayout extends SingleChildLayoutDelegate {
  const _PopoverLayout(this.anchor, this.side, this.alignEnd, this.gap);

  final Rect anchor;
  final PopoverSide side;
  final bool alignEnd;
  final double gap;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      BoxConstraints.loose(
        constraints.biggest,
      ).deflate(const EdgeInsets.all(popupMargin));

  @override
  Offset getPositionForChild(Size size, Size child) {
    double clampX(double x) => x.clamp(
      popupMargin,
      math.max(popupMargin, size.width - popupMargin - child.width),
    );
    double clampY(double y) => y.clamp(
      popupMargin,
      math.max(popupMargin, size.height - popupMargin - child.height),
    );
    final alongX = alignEnd ? anchor.right - child.width : anchor.left;
    final alongY = alignEnd ? anchor.bottom - child.height : anchor.top;
    final below = anchor.bottom + gap;
    final above = anchor.top - gap - child.height;
    final right = anchor.right + gap;
    final left = anchor.left - gap - child.width;
    final fitsBelow = below + child.height <= size.height - popupMargin;
    final fitsAbove = above >= popupMargin;
    final fitsRight = right + child.width <= size.width - popupMargin;
    final fitsLeft = left >= popupMargin;
    return switch (side) {
      PopoverSide.below => Offset(
        clampX(alongX),
        fitsBelow || !fitsAbove ? clampY(below) : above,
      ),
      PopoverSide.above => Offset(
        clampX(alongX),
        fitsAbove || !fitsBelow ? clampY(above) : below,
      ),
      PopoverSide.right => Offset(
        fitsRight || !fitsLeft ? clampX(right) : left,
        clampY(alongY),
      ),
      PopoverSide.left => Offset(
        fitsLeft || !fitsRight ? clampX(left) : right,
        clampY(alongY),
      ),
    };
  }

  @override
  bool shouldRelayout(_PopoverLayout old) =>
      old.anchor != anchor ||
      old.side != side ||
      old.alignEnd != alignEnd ||
      old.gap != gap;
}
