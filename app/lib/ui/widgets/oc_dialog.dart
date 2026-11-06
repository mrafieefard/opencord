import 'package:flutter/material.dart';

import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

class _ClampedCurve extends Curve {
  const _ClampedCurve(this.curve);
  final Curve curve;

  @override
  double transformInternal(double t) => curve.transform(t).clamp(0.0, 1.0);
}

class _OcDialogRoute<T> extends RawDialogRoute<T> {
  _OcDialogRoute({
    required super.pageBuilder,
    super.barrierDismissible = true,
    super.barrierColor,
    super.barrierLabel,
    required super.transitionDuration,
    required this.reverseDuration,
    super.transitionBuilder,
    super.settings,
  });

  final Duration reverseDuration;

  @override
  Duration get reverseTransitionDuration => reverseDuration;

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
}

/// Opens a dialog over a scrim on the pop spring (scale 0.97, fade) and closes
/// in 100ms fade (§5.17). Escape and a click on the scrim close it unless
/// [dismissible] is false.
Future<T?> showOcDialog<T>({
  required BuildContext context,
  required WidgetBuilder builder,
  bool dismissible = true,
}) {
  final colors = context.oc;
  final motion = OcMotion.of(context);
  final navigator = Navigator.of(context, rootNavigator: true);
  return navigator.push<T>(
    _OcDialogRoute<T>(
      pageBuilder: (context, animation, secondaryAnimation) =>
          Builder(builder: builder),
      barrierDismissible: dismissible,
      barrierLabel: 'Close',
      barrierColor: colors.scrim,
      transitionDuration: motion.reduced ? Duration.zero : OcSpring.pop.settle,
      reverseDuration: motion.reduced
          ? Duration.zero
          : const Duration(milliseconds: 100),
      transitionBuilder: (context, animation, secondaryAnimation, child) {
        if (motion.reduced) return child;
        if (animation.status == AnimationStatus.reverse) {
          return FadeTransition(
            opacity: CurvedAnimation(parent: animation, curve: Curves.easeIn),
            child: child,
          );
        }
        final curved = CurvedAnimation(
          parent: animation,
          curve: OcSpring.pop.curve,
        );
        final opacityCurve = CurvedAnimation(
          parent: animation,
          curve: _ClampedCurve(OcSpring.pop.curve),
        );
        return FadeTransition(
          opacity: opacityCurve,
          child: ScaleTransition(
            scale: Tween<double>(begin: 0.97, end: 1.0).animate(curved),
            child: child,
          ),
        );
      },
    ),
  );
}

/// The shared dialog shell (§4.11): title, content, and actions at the
/// bottom right (secondary first, then primary).
class OcDialog extends StatelessWidget {
  const OcDialog({
    super.key,
    required this.title,
    required this.child,
    this.actions = const [],
    this.width = 440,
    this.showClose = false,
  });

  final String title;
  final Widget child;
  final List<Widget> actions;
  final double width;

  /// A close button in the title row, for dialogs without a Cancel action.
  final bool showClose;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(OcSpace.s24),
        child: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: width),
          child: Material(
            type: MaterialType.transparency,
            child: Container(
              padding: const EdgeInsets.all(OcSpace.s24),
              decoration: BoxDecoration(
                color: colors.elevated,
                borderRadius: BorderRadius.circular(OcRadius.dialog),
                border: Border.all(color: colors.border),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Semantics(
                          header: true,
                          child: Text(
                            title,
                            style: OcText.title.copyWith(color: colors.text),
                          ),
                        ),
                      ),
                      if (showClose)
                        OcButton.ghost(
                          label: 'Close',
                          dense: true,
                          onPressed: () => Navigator.maybePop(context),
                        ),
                    ],
                  ),
                  const SizedBox(height: OcSpace.s16),
                  DefaultTextStyle(
                    style: OcText.body.copyWith(color: colors.textSecondary),
                    child: child,
                  ),
                  if (actions.isNotEmpty) ...[
                    const SizedBox(height: OcSpace.s24),
                    Wrap(
                      alignment: WrapAlignment.end,
                      spacing: OcSpace.s8,
                      runSpacing: OcSpace.s8,
                      children: actions,
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
