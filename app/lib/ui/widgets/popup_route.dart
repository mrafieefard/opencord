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

/// A transparent-barrier popup placed by a layout delegate: the base of
/// menus and popovers. A click outside or Escape closes it.
class OcPopupRoute<T> extends PopupRoute<T> {
  OcPopupRoute({
    required this.builder,
    required this.layout,
    this.alignment = Alignment.topLeft,
    Duration? duration,
    this.reducedMotion = false,
    required this.themes,
  }) : duration =
           duration ?? (reducedMotion ? Duration.zero : OcSpring.pop.settle);

  final WidgetBuilder builder;
  final SingleChildLayoutDelegate layout;
  final Alignment alignment;
  final Duration duration;
  final bool reducedMotion;
  final CapturedThemes themes;

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
      content = AnimatedBuilder(
        animation: curved,
        builder: (context, child) {
          final t = curved.value;
          final dy = -2.0 * (1.0 - t);
          return Transform.translate(offset: Offset(0, dy), child: child);
        },
        child: FadeTransition(
          opacity: opacityCurve,
          child: ScaleTransition(
            alignment: alignment,
            scale: Tween<double>(begin: 0.97, end: 1.0).animate(curved),
            child: content,
          ),
        ),
      );
    }
    return themes.wrap(
      CustomSingleChildLayout(delegate: layout, child: content),
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
/// theme so a menu opened from a light surface stays light.
Future<T?> pushPopup<T>(
  BuildContext context, {
  required WidgetBuilder builder,
  required SingleChildLayoutDelegate layout,
  Alignment alignment = Alignment.topLeft,
  Duration? duration,
}) {
  final navigator = Navigator.of(context, rootNavigator: true);
  final motion = OcMotion.of(context);
  return navigator.push<T>(
    OcPopupRoute<T>(
      builder: builder,
      layout: layout,
      alignment: alignment,
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
