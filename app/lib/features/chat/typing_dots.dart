import 'package:flutter/widgets.dart';

import 'package:opencord/ui/theme/oc_motion.dart';

/// Three dots that rise and fall in turn after "Kai is typing" (§4.3).
/// With reduced motion they stand still.
class TypingDots extends StatefulWidget {
  const TypingDots({super.key, required this.color, this.size = 3.5});

  final Color color;
  final double size;

  @override
  State<TypingDots> createState() => _TypingDotsState();
}

class _TypingDotsState extends State<TypingDots>
    with SingleTickerProviderStateMixin {
  late final _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1200),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final still = OcMotion.of(context).morph == Duration.zero;
    if (still) {
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

  /// How far dot [index] is through its rise, 0–1.
  double _lift(int index) {
    final phase = (_controller.value - index * 0.18) % 1;
    return phase < 0.4 ? (phase < 0.2 ? phase / 0.2 : (0.4 - phase) / 0.2) : 0;
  }

  @override
  Widget build(BuildContext context) {
    final size = widget.size;
    return ExcludeSemantics(
      child: AnimatedBuilder(
        animation: _controller,
        builder: (context, _) => Row(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            for (var i = 0; i < 3; i++)
              Padding(
                padding: EdgeInsets.only(
                  left: i == 0 ? 2 : size * 0.7,
                  bottom: 2 + _lift(i) * size,
                ),
                child: Opacity(
                  opacity: 0.45 + 0.55 * _lift(i),
                  child: Container(
                    width: size,
                    height: size,
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
}
