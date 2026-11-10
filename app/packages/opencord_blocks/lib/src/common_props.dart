import 'dart:math' as math;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'theme.dart';

/// Blocks that take the container props (`opencord_plugins::blocks::CONTAINERS`).
const containerBlocks = [
  'row',
  'column',
  'stack',
  'grid',
  'scroll',
  'box',
  'list',
];

/// A prop that should be a number; null when it isn't one (the core already
/// refused anything else, so this is only absent).
double? numberProp(Object? value) =>
    value is num && value.isFinite ? value.toDouble() : null;

/// `padding`: a number, or all four sides (the core sends `{x, y}` as four).
EdgeInsets? insetsProp(Object? value) {
  if (numberProp(value) case final all?) return EdgeInsets.all(all);
  if (value is! Map<String, Object?>) return null;
  double side(String name) => numberProp(value[name]) ?? 0;
  return EdgeInsets.fromLTRB(
    side('left'),
    side('top'),
    side('right'),
    side('bottom'),
  );
}

/// Draws the container props around [content]: `background` (a colour or a
/// gradient), `border` `{width, color}` (inside the block's size, like CSS's
/// border-box), `radius` (clipping the content too) and `shadow`.
Widget decorate(BlocksTheme theme, Map<String, Object?> props, Widget content) {
  final background = props['background'];
  final radius = numberProp(props['radius']) ?? 0;
  final border = switch (props['border']) {
    final Map<String, Object?> stroke => Border.all(
      width: numberProp(stroke['width']) ?? 1,
      color: theme.resolveColor(stroke['color']) ?? theme.colors['border']!,
    ),
    _ => null,
  };
  final shadow = theme.shadows[props['shadow']];
  if (background == null && border == null && radius == 0 && shadow == null) {
    return content;
  }
  final corners = radius > 0 ? BorderRadius.circular(radius) : null;
  var inside = content;
  if (border != null) {
    inside = Padding(padding: border.dimensions, child: inside);
  }
  if (corners != null) inside = ClipRRect(borderRadius: corners, child: inside);
  return DecoratedBox(
    decoration: BoxDecoration(
      color: theme.resolveColor(background),
      gradient: theme.resolveGradient(background),
      border: border,
      borderRadius: corners,
      boxShadow: shadow,
    ),
    child: inside,
  );
}

/// `width`/`height` and their `min_`/`max_`: a number, `fill` (all the room
/// given; content's size where the room has no end) or `fit` (the content's
/// size even where the parent would stretch it). Absent, the block takes
/// what its parent gives, unless [stretched] makes it `fill`.
@immutable
class AxisSize {
  const AxisSize._(this._mode, this._value, this.min, this.max);

  factory AxisSize.read(
    Map<String, Object?> props,
    String name, {
    bool stretched = false,
  }) {
    final value = props[name];
    final mode = switch (value) {
      num() => _Mode.exact,
      'fill' => _Mode.fill,
      'fit' => _Mode.fit,
      _ => stretched ? _Mode.fill : _Mode.free,
    };
    return AxisSize._(
      mode,
      numberProp(value) ?? 0,
      numberProp(props['min_$name']),
      numberProp(props['max_$name']),
    );
  }

  final _Mode _mode;
  final double _value;
  final double? min;
  final double? max;

  bool get isFree => _mode == _Mode.free && min == null && max == null;

  /// The child's range on this axis, from the parent's [lo]–[hi]. `max`
  /// beats the size and `min` beats `max`, as in CSS; the parent's own
  /// maximum beats them all.
  (double, double) resolve(double lo, double hi) {
    var low = lo;
    var high = hi;
    switch (_mode) {
      case _Mode.exact:
        low = high = _value.clamp(lo, hi);
      case _Mode.fill:
        if (hi.isFinite) low = hi;
      case _Mode.fit:
        low = 0;
      case _Mode.free:
        break;
    }
    if (max case final most?) {
      high = math.min(high, most);
      low = math.min(low, high);
    }
    if (min case final least?) {
      low = math.max(low, math.min(least, hi));
      high = math.max(high, low);
    }
    return (low, high);
  }

  /// The size this axis asks for given the content's [content] size, for
  /// intrinsic sizing.
  double intrinsic(double content) {
    var size = _mode == _Mode.exact ? _value : content;
    if (max case final most?) size = math.min(size, most);
    if (min case final least?) size = math.max(size, least);
    return size;
  }

  @override
  bool operator ==(Object other) =>
      other is AxisSize &&
      other._mode == _mode &&
      other._value == _value &&
      other.min == min &&
      other.max == max;

  @override
  int get hashCode => Object.hash(_mode, _value, min, max);
}

enum _Mode { free, exact, fill, fit }

/// Sizes its child by [width] and [height] (see [AxisSize]); the child sits
/// at its top left when it's smaller than the room the parent forces.
class BlockSize extends SingleChildRenderObjectWidget {
  const BlockSize({
    super.key,
    required this.width,
    required this.height,
    super.child,
  });

  final AxisSize width;
  final AxisSize height;

  @override
  RenderBlockSize createRenderObject(BuildContext context) =>
      RenderBlockSize(width, height);

  @override
  void updateRenderObject(BuildContext context, RenderBlockSize renderObject) {
    renderObject
      ..width = width
      ..height = height;
  }
}

class RenderBlockSize extends RenderProxyBox {
  RenderBlockSize(this._width, this._height);

  AxisSize _width;
  AxisSize get width => _width;
  set width(AxisSize value) {
    if (value == _width) return;
    _width = value;
    markNeedsLayout();
  }

  AxisSize _height;
  AxisSize get height => _height;
  set height(AxisSize value) {
    if (value == _height) return;
    _height = value;
    markNeedsLayout();
  }

  BoxConstraints _inner(BoxConstraints outer) {
    final (minWidth, maxWidth) = _width.resolve(outer.minWidth, outer.maxWidth);
    final (minHeight, maxHeight) = _height.resolve(
      outer.minHeight,
      outer.maxHeight,
    );
    return BoxConstraints(
      minWidth: minWidth,
      maxWidth: maxWidth,
      minHeight: minHeight,
      maxHeight: maxHeight,
    );
  }

  @override
  void performLayout() {
    final inner = _inner(constraints);
    final child = this.child;
    if (child == null) {
      size = constraints.constrain(inner.smallest);
      return;
    }
    child.layout(inner, parentUsesSize: true);
    size = constraints.constrain(child.size);
  }

  @override
  Size computeDryLayout(BoxConstraints constraints) {
    final inner = _inner(constraints);
    return constraints.constrain(child?.getDryLayout(inner) ?? inner.smallest);
  }

  @override
  double computeMinIntrinsicWidth(double height) =>
      _width.intrinsic(child?.getMinIntrinsicWidth(height) ?? 0);

  @override
  double computeMaxIntrinsicWidth(double height) =>
      _width.intrinsic(child?.getMaxIntrinsicWidth(height) ?? 0);

  @override
  double computeMinIntrinsicHeight(double width) =>
      _height.intrinsic(child?.getMinIntrinsicHeight(width) ?? 0);

  @override
  double computeMaxIntrinsicHeight(double width) =>
      _height.intrinsic(child?.getMaxIntrinsicHeight(width) ?? 0);
}
