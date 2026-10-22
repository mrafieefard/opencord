import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

/// A transparent-barrier popup placed by a layout delegate: the base of
/// menus and popovers. A click outside or Escape closes it.
class OcPopupRoute<T> extends PopupRoute<T> {
  OcPopupRoute({
    required this.builder,
    required this.layout,
    required this.duration,
    required this.themes,
  });

  final WidgetBuilder builder;
  final SingleChildLayoutDelegate layout;
  final Duration duration;
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
  Widget buildPage(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
  ) {
    return themes.wrap(
      CustomSingleChildLayout(
        delegate: layout,
        child: Builder(builder: builder),
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
    return FadeTransition(
      opacity: CurvedAnimation(parent: animation, curve: Curves.easeOut),
      child: child,
    );
  }
}

/// Pushes [route] above everything, including dialogs, with the caller's
/// theme so a menu opened from a light surface stays light.
Future<T?> pushPopup<T>(
  BuildContext context, {
  required WidgetBuilder builder,
  required SingleChildLayoutDelegate layout,
  required Duration duration,
}) {
  final navigator = Navigator.of(context, rootNavigator: true);
  return navigator.push<T>(
    OcPopupRoute<T>(
      builder: builder,
      layout: layout,
      duration: duration,
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
