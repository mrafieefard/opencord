import 'dart:async';

import 'package:flutter/material.dart';

import '../common_props.dart';
import '../view.dart';

/// `slider` (design §12.2): numeric range slider; throttles `change` events
/// to at most 20 a second while dragging and sends `change_end` on release.
final sliderBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _SliderWidget(key: ValueKey<int>(b.node.id), build: b);
}

class _SliderWidget extends StatefulWidget {
  const _SliderWidget({super.key, required this.build});

  final BlockBuild build;

  @override
  State<_SliderWidget> createState() => _SliderWidgetState();
}

class _SliderWidgetState extends State<_SliderWidget> {
  double _value = 0.0;
  Timer? _throttleTimer;
  double? _pendingValue;
  bool _dragging = false;

  @override
  void initState() {
    super.initState();
    _value = _readInitial();
  }

  @override
  void didUpdateWidget(_SliderWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_dragging && _pendingValue == null) {
      _value = _readInitial();
    }
  }

  @override
  void dispose() {
    _throttleTimer?.cancel();
    super.dispose();
  }

  double _readInitial() {
    final props = widget.build.props;
    final val = numberProp(props['value']) ?? 0.0;
    final min = numberProp(props['min']) ?? 0.0;
    final max = numberProp(props['max']) ?? 1.0;
    return val.clamp(min, max);
  }

  void _onChangeStart(double startVal) {
    _dragging = true;
  }

  void _onChanged(double nextVal) {
    _dragging = true;
    setState(() => _value = nextVal);
    final node = widget.build.node;
    if (!node.handles('change')) return;

    if (_throttleTimer == null) {
      widget.build.send('change', {'value': nextVal});
      _throttleTimer = Timer(const Duration(milliseconds: 50), () {
        _throttleTimer = null;
        if (!mounted) return;
        final pending = _pendingValue;
        _pendingValue = null;
        if (pending != null) {
          _onChanged(pending);
        }
      });
    } else {
      _pendingValue = nextVal;
    }
  }

  void _onChangeEnd(double endVal) {
    _dragging = false;
    _throttleTimer?.cancel();
    _throttleTimer = null;
    final pending = _pendingValue;
    _pendingValue = null;
    final node = widget.build.node;
    if (pending != null && node.handles('change')) {
      widget.build.send('change', {'value': pending});
    }
    if (node.handles('change_end')) {
      widget.build.send('change_end', {'value': endVal});
    }
  }

  @override
  Widget build(BuildContext context) {
    final props = widget.build.props;
    final min = numberProp(props['min']) ?? 0.0;
    final max = numberProp(props['max']) ?? 1.0;
    final step = numberProp(props['step']);
    final disabled = props['disabled'] == true;
    final theme = widget.build.theme;

    final safeMin = min < max ? min : 0.0;
    final safeMax = max > safeMin ? max : 1.0;
    final current = _value.clamp(safeMin, safeMax);

    int? divisions;
    if (step != null && step > 0) {
      divisions = ((safeMax - safeMin) / step).round().clamp(1, 1000);
    }

    Widget slider = Material(
      type: MaterialType.transparency,
      child: SliderTheme(
        data: SliderThemeData(
          activeTrackColor: theme.colors['accent']!,
          inactiveTrackColor: theme.colors['border']!,
          thumbColor: theme.colors['accent']!,
          overlayColor: theme.colors['accent']!.withAlpha(32),
          trackHeight: 4,
          thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 7),
        ),
        child: Slider(
          value: current,
          min: safeMin,
          max: safeMax,
          divisions: divisions,
          onChangeStart: !disabled ? _onChangeStart : null,
          onChanged: !disabled ? _onChanged : null,
          onChangeEnd: !disabled ? _onChangeEnd : null,
        ),
      ),
    );

    if (disabled) {
      slider = Opacity(opacity: 0.5, child: slider);
    }

    return slider;
  }
}
