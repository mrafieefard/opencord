import 'package:flutter/widgets.dart';

import 'package:opencord/ui/theme/oc_motion.dart';

/// Three dots fading in turn while a command is thinking (§26.6). With
/// reduced motion they stand still.
class ThinkingDots extends StatefulWidget {
  const ThinkingDots({super.key, required this.color});

  final Color color;

  @override
  State<ThinkingDots> createState() => _ThinkingDotsState();
}

class _ThinkingDotsState extends State<ThinkingDots>
    with SingleTickerProviderStateMixin {
  static const _size = 5.0;
  static const _dim = 0.25;
  static const _still = 0.6;

  /// Each dot starts this far through the loop after the one before.
  static const _delay = 0.18 / 1.1;

  late final _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1100),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (OcMotion.of(context).reduced) {
      _controller.stop();
    } else if (!_controller.isAnimating) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  /// Up to full at 30 % of the loop, back down by 70 %.
  double _opacity(int dot) {
    if (OcMotion.of(context).reduced) return _still;
    final phase = (_controller.value - dot * _delay) % 1;
    final rise = phase < 0.3
        ? phase / 0.3
        : phase < 0.7
        ? (0.7 - phase) / 0.4
        : 0.0;
    return _dim + (1 - _dim) * Curves.easeInOut.transform(rise);
  }

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: AnimatedBuilder(
      animation: _controller,
      builder: (context, _) => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (var dot = 0; dot < 3; dot++)
            Padding(
              padding: EdgeInsets.only(left: dot == 0 ? 6 : 3),
              child: Opacity(
                opacity: _opacity(dot),
                child: Container(
                  width: _size,
                  height: _size,
                  decoration: BoxDecoration(
                    color: widget.color,
                    shape: BoxShape.circle,
                  ),
                ),
              ),
            ),
        ],
      ),
    ),
  );
}
