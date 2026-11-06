import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/motion/spring_value.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/oc_float.dart';

/// A drawer overlay over the shell (§5.18).
///
/// Right drawer: floats 8 px from the right and bottom, opens on [OcSpring.pane],
/// closes on [OcSpring.paneOut].
/// Left drawer: floats at [inset] (78 px) from the left, opens on [OcSpring.pane]
/// from -24 px with a fade (100 ms), leaves on 120 ms easeIn.
/// The scrim fades in over 120 ms and goes as soon as the drawer starts to
/// close, as the prototype's does.
class ShellDrawer extends StatefulWidget {
  const ShellDrawer({
    super.key,
    required this.fromRight,
    required this.onClose,
    required this.width,
    required this.child,
    this.inset = 0,
    this.onClosing,
  });

  final bool fromRight;

  /// Called once the way out has finished.
  final VoidCallback onClose;

  /// Called as the way out starts.
  final VoidCallback? onClosing;
  final double width;
  final Widget child;
  final double inset;

  @override
  State<ShellDrawer> createState() => ShellDrawerState();
}

class ShellDrawerState extends State<ShellDrawer>
    with SingleTickerProviderStateMixin {
  static const _leftFade = Duration(milliseconds: 100);

  final _scope = FocusScopeNode(debugLabel: 'drawer');
  late final AnimationController _controller;
  bool _closing = false;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController.unbounded(vsync: this, value: 0.0);
    _controller.addListener(() {
      if (mounted) setState(() {});
    });
    _controller.animateWith(OcSpring.pane.simulation(0.0, 1.0, 0.0));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _scope.requestFocus();
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.disableAnimationsOf(context)) {
      _controller.value = 1.0;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _scope.dispose();
    super.dispose();
  }

  /// Plays the way out, then calls [ShellDrawer.onClose]. Anything that shuts
  /// a drawer comes through here, or it would vanish.
  void close() {
    if (_closing || !mounted) return;
    setState(() => _closing = true);
    widget.onClosing?.call();
    if (MediaQuery.disableAnimationsOf(context)) {
      widget.onClose();
      return;
    }
    if (widget.fromRight) {
      _controller
          .animateWith(
            OcSpring.paneOut.simulation(
              _controller.value,
              0.0,
              _controller.velocity,
            ),
          )
          .whenCompleteOrCancel(_closed);
    } else {
      _controller
          .animateTo(
            0.0,
            duration: const Duration(milliseconds: 120),
            curve: Curves.easeIn,
          )
          .whenCompleteOrCancel(_closed);
    }
  }

  /// Turning back is what cancels a way out, and then nothing has closed.
  void _closed() {
    if (mounted && _closing) widget.onClose();
  }

  /// Turns back on the way out, from where it has got to.
  void reopen() {
    if (!_closing || !mounted) return;
    setState(() => _closing = false);
    _controller.animateWith(
      OcSpring.pane.simulation(_controller.value, 1.0, _controller.velocity),
    );
    _scope.requestFocus();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final progress = _controller.value;
    final fromRight = widget.fromRight;

    final motion = OcMotion.of(context);

    final distance = fromRight ? widget.width + OcFloats.gap + 24.0 : 24.0;
    final offset = (1.0 - progress) * distance;

    return Positioned.fill(
      child: Shortcuts(
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.escape): DismissIntent(),
        },
        child: Actions(
          actions: {
            DismissIntent: CallbackAction<DismissIntent>(
              onInvoke: (_) => close(),
            ),
          },
          child: FocusScope(
            node: _scope,
            child: Stack(
              children: [
                // Scrim covering full area under titlebar.
                if (!_closing)
                  Positioned.fill(
                    child: GestureDetector(
                      key: ShellKeys.scrim,
                      behavior: HitTestBehavior.opaque,
                      onTap: close,
                      child: TweenAnimationBuilder<double>(
                        tween: Tween(begin: 0, end: colors.scrim.a * 0.6),
                        duration: motion.fade,
                        curve: OcMotion.curve,
                        builder: (context, alpha, _) => ColoredBox(
                          color: colors.scrim.withValues(alpha: alpha),
                        ),
                      ),
                    ),
                  ),
                // Drawer float.
                if (fromRight)
                  SpringValue(
                    target: widget.width,
                    spring: OcSpring.pane,
                    falling: OcSpring.paneOut,
                    builder: (context, width, float) => Positioned(
                      key: ShellKeys.side,
                      top: 0,
                      bottom: OcFloats.gap,
                      right: OcFloats.gap - offset,
                      width: width,
                      child: float!,
                    ),
                    child: OcFloat(
                      key: ShellKeys.panel,
                      color: colors.sidebar,
                      child: widget.child,
                    ),
                  )
                else
                  Positioned(
                    top: 0,
                    bottom: OcFloats.gap,
                    left: widget.inset - offset,
                    width: widget.width,
                    child: TweenAnimationBuilder<double>(
                      tween: Tween(begin: 0, end: 1),
                      duration: motion.fade == Duration.zero
                          ? Duration.zero
                          : _leftFade,
                      curve: OcMotion.curve,
                      builder: (context, fade, child) => Opacity(
                        opacity: _closing ? progress.clamp(0.0, 1.0) : fade,
                        child: child,
                      ),
                      child: KeyedSubtree(
                        key: ShellKeys.left,
                        child: widget.child,
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
