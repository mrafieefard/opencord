import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

/// Transparent space around the visible window for its shadow (Linux
/// custom frame, §3.1).
const double windowFrameMargin = 14;

/// The part of the margin next to the window that resizes it.
const double windowResizeBand = 6;

/// Corners resize diagonally within this distance of the corner.
const double windowCornerSize = 12;

const double windowCornerRadius = 12;

/// The Linux custom frame (§3.1): while the window floats, rounded corners,
/// a 1 px outline and a soft shadow drawn in a transparent margin, with
/// invisible resize handles around the edges. Without compositing the
/// corners stay square and the handles sit just inside the edges.
/// Maximized, tiled and fullscreen windows get none of it.
class WindowFrame extends ConsumerWidget {
  const WindowFrame({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final info = ref.watch(windowInfoProvider);
    final floating = ref.watch(
      windowStatusProvider.select((status) => status.floating),
    );
    if (Theme.of(context).platform != TargetPlatform.linux ||
        info.chrome != WindowChrome.custom ||
        !floating) {
      return child;
    }
    final colors = context.oc;
    final window = ref.watch(nativeWindowProvider);
    final margin = info.frameMargin;
    if (margin == 0) {
      return Stack(
        children: [
          Positioned.fill(child: child),
          ..._handles(window, outside: 0, band: windowResizeBand),
        ],
      );
    }
    final radius = BorderRadius.circular(windowCornerRadius);
    return Stack(
      children: [
        Positioned.fill(
          child: Padding(
            padding: EdgeInsets.all(margin),
            child: DecoratedBox(
              decoration: BoxDecoration(
                borderRadius: radius,
                boxShadow: OcShadows.window,
              ),
              position: DecorationPosition.background,
              child: ClipRRect(
                borderRadius: radius,
                child: DecoratedBox(
                  position: DecorationPosition.foreground,
                  decoration: BoxDecoration(
                    borderRadius: radius,
                    border: Border.all(color: colors.border),
                  ),
                  child: child,
                ),
              ),
            ),
          ),
        ),
        ..._handles(
          window,
          outside: margin - windowResizeBand,
          band: windowResizeBand,
        ),
      ],
    );
  }

  /// Resize handles: a [band] wide ring starting [outside] from the window
  /// edge, with square corners of [windowCornerSize].
  List<Widget> _handles(
    NativeWindow window, {
    required double outside,
    required double band,
  }) {
    const corner = windowCornerSize;
    Widget handle(ResizeEdge edge, MouseCursor cursor) => MouseRegion(
      cursor: cursor,
      child: Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: (event) {
          if (event.buttons == 1) window.startResize(edge);
        },
      ),
    );
    return [
      Positioned(
        left: outside + corner,
        right: outside + corner,
        top: outside,
        height: band,
        child: handle(ResizeEdge.top, SystemMouseCursors.resizeUp),
      ),
      Positioned(
        left: outside + corner,
        right: outside + corner,
        bottom: outside,
        height: band,
        child: handle(ResizeEdge.bottom, SystemMouseCursors.resizeDown),
      ),
      Positioned(
        top: outside + corner,
        bottom: outside + corner,
        left: outside,
        width: band,
        child: handle(ResizeEdge.left, SystemMouseCursors.resizeLeft),
      ),
      Positioned(
        top: outside + corner,
        bottom: outside + corner,
        right: outside,
        width: band,
        child: handle(ResizeEdge.right, SystemMouseCursors.resizeRight),
      ),
      for (final (edge, cursor, left, top) in const [
        (ResizeEdge.topLeft, SystemMouseCursors.resizeUpLeft, true, true),
        (ResizeEdge.topRight, SystemMouseCursors.resizeUpRight, false, true),
        (ResizeEdge.bottomLeft, SystemMouseCursors.resizeDownLeft, true, false),
        (
          ResizeEdge.bottomRight,
          SystemMouseCursors.resizeDownRight,
          false,
          false,
        ),
      ])
        Positioned(
          left: left ? outside : null,
          right: left ? null : outside,
          top: top ? outside : null,
          bottom: top ? null : outside,
          width: corner + band,
          height: corner + band,
          child: _CornerHandle(
            band: band,
            left: left,
            top: top,
            child: handle(edge, cursor),
          ),
        ),
    ];
  }
}

/// An L-shaped corner handle, so the window's own content under the
/// corner square keeps its clicks.
class _CornerHandle extends StatelessWidget {
  const _CornerHandle({
    required this.band,
    required this.left,
    required this.top,
    required this.child,
  });

  final double band;
  final bool left;
  final bool top;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        Positioned(
          left: 0,
          right: 0,
          top: top ? 0 : null,
          bottom: top ? null : 0,
          height: band,
          child: child,
        ),
        Positioned(
          top: 0,
          bottom: 0,
          left: left ? 0 : null,
          right: left ? null : 0,
          width: band,
          child: child,
        ),
      ],
    );
  }
}
