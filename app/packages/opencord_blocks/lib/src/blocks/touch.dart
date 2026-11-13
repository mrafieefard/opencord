import 'package:flutter/widgets.dart';

import '../theme.dart';

/// [child], with a tap target at least [BlocksTheme.minTouchSize] high and
/// wide around it; what it draws keeps its size. Wraps what a control's
/// GestureDetector holds.
Widget touchTarget(BlocksTheme theme, Widget child) {
  final size = theme.minTouchSize;
  if (size <= 0) return child;
  return ConstrainedBox(
    constraints: BoxConstraints(minHeight: size, minWidth: size),
    child: Center(widthFactor: 1, heightFactor: 1, child: child),
  );
}
