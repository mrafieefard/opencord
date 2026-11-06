import 'dart:math' as math;

import 'package:opencord/ui/theme/oc_metrics.dart';

/// The responsive rules of §2.2 for desktop window widths.
enum ShellLayout {
  /// ≥ 1180 px: all four columns.
  wide,

  /// 980–1179 px: the member panel only as an overlay.
  medium,

  /// 760–979 px: a narrower sidebar.
  narrow,

  /// < 760 px: the sidebar becomes an overlay too.
  compact;

  static ShellLayout forWidth(double width) => width >= OcBreakpoint.fourColumns
      ? wide
      : width >= OcBreakpoint.narrowSidebar
      ? medium
      : width >= OcBreakpoint.collapsedSidebar
      ? narrow
      : compact;

  double get sidebarWidth => this == wide || this == medium
      ? OcSize.sidebarWidth
      : OcSize.sidebarNarrowWidth;

  /// Width of the left column drawer when in compact mode (§5.18).
  double leftDrawerWidth(double windowWidth) => math.min(
    OcSize.sidebarWidth,
    math.max(
      OcFloats.leftMin,
      windowWidth - OcFloats.rail - OcFloats.gap * 4 - 380,
    ),
  );

  bool get sidebarInline => this != compact;

  bool get membersInline => this == wide;
}
