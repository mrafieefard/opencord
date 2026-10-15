import 'package:flutter/rendering.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

/// Tells [onChanged] its child's height after each layout that changes it,
/// for something elsewhere that has to match it. The news comes after the
/// frame, so whoever listens follows a frame later.
class MeasuredHeight extends SingleChildRenderObjectWidget {
  const MeasuredHeight({super.key, required this.onChanged, super.child});

  final ValueChanged<double> onChanged;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      RenderMeasuredHeight(onChanged);

  @override
  void updateRenderObject(
    BuildContext context,
    RenderMeasuredHeight renderObject,
  ) => renderObject.onChanged = onChanged;
}

class RenderMeasuredHeight extends RenderProxyBox {
  RenderMeasuredHeight(this.onChanged);

  ValueChanged<double> onChanged;
  double? _reported;

  @override
  void performLayout() {
    super.performLayout();
    final height = size.height;
    if (height == _reported) return;
    _reported = height;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      if (attached) onChanged(height);
    });
  }
}
