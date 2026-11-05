import 'package:flutter/material.dart';

import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

class _ClampedCurve extends Curve {
  const _ClampedCurve(this.curve);
  final Curve curve;

  @override
  double transformInternal(double t) => curve.transform(t).clamp(0.0, 1.0);
}

/// Where a popup grows from: its corner nearest the trigger, found by its
/// layout once it knows where the popup lands (§5.17).
class _Origin {
  Alignment corner = Alignment.topLeft;
}

/// [inner]'s layout, noting the popup's corner nearest [trigger].
class _GrowFrom extends SingleChildLayoutDelegate {
  _GrowFrom(this.inner, this.trigger, this.origin);

  final SingleChildLayoutDelegate inner;
  final Offset trigger;
  final _Origin origin;

  @override
  Size getSize(BoxConstraints constraints) => inner.getSize(constraints);

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      inner.getConstraintsForChild(constraints);

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final at = inner.getPositionForChild(size, childSize);
    final popup = at & childSize;
    origin.corner = Alignment(
      trigger.dx <= popup.center.dx ? -1 : 1,
      trigger.dy <= popup.center.dy ? -1 : 1,
    );
    return at;
  }

  @override
  bool shouldRelayout(_GrowFrom old) =>
      old.trigger != trigger ||
      old.inner.runtimeType != inner.runtimeType ||
      inner.shouldRelayout(old.inner);
}

/// A transparent-barrier popup placed by a layout delegate: the base of
/// menus and popovers. A click outside or Escape closes it.
class OcPopupRoute<T> extends PopupRoute<T> {
  OcPopupRoute({
    required this.builder,
    required this.layout,
    this.trigger,
    Duration? duration,
    this.reducedMotion = false,
    required this.themes,
  }) : duration =
           duration ?? (reducedMotion ? Duration.zero : OcSpring.pop.settle);

  final WidgetBuilder builder;
  final SingleChildLayoutDelegate layout;

  /// Where it was opened from, global: it grows from its corner nearest
  /// this, wherever [layout] puts it.
  final Offset? trigger;
  final Duration duration;
  final bool reducedMotion;
  final CapturedThemes themes;
  final _origin = _Origin();

  @override
  Color? get barrierColor => null;

  @override
  bool get barrierDismissible => true;

  @override
  String? get barrierLabel => 'Close';

  @override
  Duration get transitionDuration => duration;

  @override
  Duration get reverseTransitionDuration => Duration.zero;

  @override
  TickerFuture didPush() {
    if (transitionDuration == Duration.zero) {
      controller?.value = 1.0;
      return TickerFuture.complete();
    }
    return super.didPush();
  }

  @override
  bool didPop(T? result) {
    final popped = super.didPop(result);
    if (reverseTransitionDuration == Duration.zero) {
      controller?.value = 0.0;
      controller?.stop();
    }
    return popped;
  }

  @override
  Widget buildPage(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
  ) {
    Widget content = Builder(builder: builder);
    if (!reducedMotion && duration > Duration.zero) {
      final curved = CurvedAnimation(
        parent: animation,
        curve: OcSpring.pop.curve,
      );
      final opacityCurve = CurvedAnimation(
        parent: animation,
        curve: _ClampedCurve(OcSpring.pop.curve),
      );
      final scale = Tween<double>(begin: 0.97, end: 1.0).animate(curved);
      content = AnimatedBuilder(
        animation: curved,
        builder: (context, child) {
          final t = curved.value;
          final dy = -2.0 * (1.0 - t);
          return Transform.translate(
            offset: Offset(0, dy),
            // The corner is read every frame: the layout finds it after the
            // first frame is built, while the popup is still transparent.
            child: ScaleTransition(
              alignment: _origin.corner,
              scale: scale,
              child: child,
            ),
          );
        },
        child: FadeTransition(opacity: opacityCurve, child: content),
      );
    }
    final at = trigger;
    return themes.wrap(
      CustomSingleChildLayout(
        delegate: at == null ? layout : _GrowFrom(layout, at, _origin),
        child: content,
      ),
    );
  }

  @override
  Widget buildTransitions(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) {
    return child;
  }
}

/// Pushes [route] above everything, including dialogs, with the caller's
/// theme so a menu opened from a light surface stays light. It grows from
/// its corner nearest [trigger] (global), or its top left without one.
Future<T?> pushPopup<T>(
  BuildContext context, {
  required WidgetBuilder builder,
  required SingleChildLayoutDelegate layout,
  Offset? trigger,
  Duration? duration,
}) {
  final navigator = Navigator.of(context, rootNavigator: true);
  final motion = OcMotion.of(context);
  return navigator.push<T>(
    OcPopupRoute<T>(
      builder: builder,
      layout: layout,
      trigger: trigger,
      duration: duration,
      reducedMotion: motion.reduced,
      themes: InheritedTheme.capture(from: context, to: navigator.context),
    ),
  );
}

/// The global rectangle a widget occupies.
Rect globalRectOf(BuildContext context) {
  final box = context.findRenderObject()! as RenderBox;
  return box.localToGlobal(Offset.zero) & box.size;
}

/// Space kept between popups and the window edges.
const double popupMargin = 8;

/// The elevated surface shared by menus and popovers.
BoxDecoration popupDecoration(OcColors colors) => BoxDecoration(
  color: colors.elevated,
  borderRadius: BorderRadius.circular(OcRadius.menu),
  border: Border.all(color: colors.border),
  boxShadow: OcShadows.menu(colors),
);
