import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/oc_float.dart';

/// A drawer overlay over the shell (§5.18).
///
/// Right drawer: floats 8 px from the right and bottom, opens on [OcSpring.pane],
/// closes on [OcSpring.paneOut].
/// Left drawer: floats at [inset] (78 px) from the left, opens on [OcSpring.pane]
/// from -24 px with a fade (100 ms), leaves on 120 ms easeIn.
class ShellDrawer extends StatefulWidget {
  const ShellDrawer({
    super.key,
    required this.fromRight,
    required this.onClose,
    required this.width,
    required this.child,
    this.inset = 0,
  });

  final bool fromRight;
  final VoidCallback onClose;
  final double width;
  final Widget child;
  final double inset;

  @override
  State<ShellDrawer> createState() => _ShellDrawerState();
}

class _ShellDrawerState extends State<ShellDrawer>
    with SingleTickerProviderStateMixin {
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

  void _close() {
    if (_closing) return;
    _closing = true;
    if (!mounted) return;
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
          .whenCompleteOrCancel(() {
            if (mounted) widget.onClose();
          });
    } else {
      _controller
          .animateTo(
            0.0,
            duration: const Duration(milliseconds: 120),
            curve: Curves.easeIn,
          )
          .whenCompleteOrCancel(() {
            if (mounted) widget.onClose();
          });
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final progress = _controller.value;
    final fromRight = widget.fromRight;

    final scrimAlpha = (colors.scrim.a * 0.6 * progress.clamp(0.0, 1.0));
    final scrimColor = colors.scrim.withValues(alpha: scrimAlpha);

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
              onInvoke: (_) => _close(),
            ),
          },
          child: FocusScope(
            node: _scope,
            child: Stack(
              children: [
                // Scrim covering full area under titlebar.
                Positioned.fill(
                  child: GestureDetector(
                    key: ShellKeys.scrim,
                    behavior: HitTestBehavior.opaque,
                    onTap: _close,
                    child: ColoredBox(color: scrimColor),
                  ),
                ),
                // Drawer float.
                if (fromRight)
                  Positioned(
                    key: ShellKeys.side,
                    top: 0,
                    bottom: OcFloats.gap,
                    right: OcFloats.gap - offset,
                    width: widget.width,
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
                    child: Opacity(
                      opacity: progress.clamp(0.0, 1.0),
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
