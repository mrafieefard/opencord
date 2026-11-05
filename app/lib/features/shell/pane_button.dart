import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// The button in the main page's notch that opens the right pane (§5.9). It
/// goes while [open], towards its top right corner.
///
/// Sits in a [OcFloats.notch] square at main's top right corner. [notch] is
/// the cut's side now, which the checks measure.
class PaneButton extends StatefulWidget {
  const PaneButton({
    super.key,
    required this.notch,
    required this.open,
    required this.onOpen,
  });

  final ValueListenable<double> notch;
  final bool open;
  final VoidCallback onOpen;

  @override
  State<PaneButton> createState() => _PaneButtonState();
}

class _PaneButtonState extends State<PaneButton>
    with SingleTickerProviderStateMixin {
  static const _goneScale = 0.6;
  static const _pressedScale = 0.97;
  static const _iconSize = 20.0;
  static const _radius = BorderRadius.only(
    topLeft: Radius.circular(10),
    topRight: Radius.circular(OcFloats.radius),
    bottomRight: Radius.circular(10),
    bottomLeft: Radius.circular(OcFloats.radius),
  );

  late final _scale = AnimationController.unbounded(
    vsync: this,
    value: widget.open ? _goneScale : 1,
  );

  @override
  void didUpdateWidget(PaneButton oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.open == oldWidget.open) return;
    final motion = OcMotion.of(context);
    if (MediaQuery.disableAnimationsOf(context)) {
      _scale.value = widget.open ? _goneScale : 1;
    } else if (widget.open) {
      _scale.animateTo(
        _goneScale,
        duration: motion.exit,
        curve: OcMotion.curveIn,
      );
    } else {
      _scale.animateWith(
        OcSpring.pop.simulation(_scale.value, 1, _scale.velocity),
      );
    }
  }

  @override
  void dispose() {
    _scale.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final motion = OcMotion.of(context);
    return SizedBox.square(
      dimension: OcFloats.notch,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Align(
            alignment: Alignment.topRight,
            child: ValueListenableBuilder<double>(
              valueListenable: widget.notch,
              builder: (context, notch, _) => notch > 0
                  ? SizedBox.square(key: ShellKeys.notch, dimension: notch)
                  : const SizedBox.shrink(),
            ),
          ),
          Positioned(
            top: OcFloats.paneButtonInset,
            right: OcFloats.paneButtonInset,
            child: AnimatedOpacity(
              opacity: widget.open ? 0 : 1,
              duration: widget.open ? motion.exit : motion.fadeQuick,
              curve: widget.open ? OcMotion.curveIn : OcMotion.curve,
              child: AnimatedBuilder(
                animation: _scale,
                builder: (context, button) =>
                    widget.open && _scale.value <= _goneScale
                    ? const SizedBox.shrink()
                    : Transform.scale(
                        scale: _scale.value,
                        alignment: Alignment.topRight,
                        child: button,
                      ),
                child: IgnorePointer(
                  ignoring: widget.open,
                  child: ExcludeFocus(
                    excluding: widget.open,
                    child: _button(context, motion),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _button(BuildContext context, OcMotion motion) {
    final colors = context.oc;
    const tooltip = 'Open the right pane';
    return Tooltip(
      message: tooltip,
      excludeFromSemantics: true,
      child: Hoverable(
        onTap: widget.onOpen,
        semanticLabel: tooltip,
        focusRadius: _radius,
        builder: (context, state) => AnimatedScale(
          scale: state.pressed ? _pressedScale : 1,
          duration: motion.press,
          curve: OcMotion.curve,
          child: AnimatedContainer(
            key: ShellKeys.paneTab,
            duration: motion.hover,
            curve: OcMotion.curve,
            width: OcFloats.paneButton,
            height: OcFloats.paneButton,
            decoration: BoxDecoration(
              color: state.active ? colors.selected : colors.sidebar,
              border: Border.all(color: colors.border),
              borderRadius: _radius,
              boxShadow: OcShadows.float(colors),
            ),
            child: Icon(
              OcIcons.dockToLeft,
              size: _iconSize,
              color: state.active ? colors.text : colors.textSecondary,
            ),
          ),
        ),
      ),
    );
  }
}
