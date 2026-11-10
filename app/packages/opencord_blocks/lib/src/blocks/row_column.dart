import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `row` and `column` (design §12.1): `gap`, `align` on the main axis,
/// `cross` on the other, and `wrap`. Children with `visible = false` take no
/// room and no gap; `flex` shares the free room by ratio, and a main-axis
/// `fill` takes a share of 1; neither applies where the main axis has no
/// end (a row inside a row), so a plugin can't break the layout with them.
final rowBlock = BlockRenderer((b) => _flex(b, Axis.horizontal));
final columnBlock = BlockRenderer((b) => _flex(b, Axis.vertical));

/// `flex` is a number; Flutter's is whole, so ratios keep three decimals.
const _flexScale = 1000;

Widget _flex(BlockBuild b, Axis axis) {
  final props = b.props;
  final gap = numberProp(props['gap']) ?? 0;
  final align = props['align'] as String? ?? 'start';
  final cross = props['cross'] as String? ?? 'start';
  final wrap = props['wrap'] == true;
  final crossAxis = flipAxis(axis);
  final (mainName, crossName) = axis == Axis.horizontal
      ? ('width', 'height')
      : ('height', 'width');
  final stretch = cross == 'stretch' && !wrap;
  final controller = b.controller;
  final ids = b.node.children;

  // The children's own props decide where they sit (shown, flex, stretch),
  // so this redraws when any of them changes.
  return ListenableBuilder(
    listenable: Listenable.merge([for (final id in ids) controller.watch(id)]),
    builder: (context, _) {
      final children = <Widget>[];
      for (final id in ids) {
        final child = controller.node(id);
        if (child == null || child.props['visible'] == false) continue;
        final placed = BlockWidget(
          key: wrap ? ValueKey<int>(id) : null,
          id: id,
          stretch: stretch && child.props[crossName] == null ? crossAxis : null,
        );
        if (wrap) {
          children.add(placed);
          continue;
        }
        final flex = numberProp(child.props['flex']) ?? 0;
        final share = flex > 0
            ? (flex * _flexScale).round().clamp(1, 1 << 40)
            : (child.props[mainName] == 'fill' ? _flexScale : 0);
        children.add(
          share > 0
              ? Flexible(
                  key: ValueKey<int>(id),
                  flex: share,
                  fit: FlexFit.tight,
                  child: placed,
                )
              : KeyedSubtree(key: ValueKey<int>(id), child: placed),
        );
      }
      if (wrap) {
        return Wrap(
          direction: axis,
          spacing: gap,
          runSpacing: gap,
          alignment: switch (align) {
            'center' => WrapAlignment.center,
            'end' => WrapAlignment.end,
            'space_between' => WrapAlignment.spaceBetween,
            'space_around' => WrapAlignment.spaceAround,
            _ => WrapAlignment.start,
          },
          crossAxisAlignment: switch (cross) {
            'center' => WrapCrossAlignment.center,
            'end' => WrapCrossAlignment.end,
            _ => WrapCrossAlignment.start,
          },
          children: children,
        );
      }
      final flex = _BlockFlex(
        direction: axis,
        spacing: gap,
        mainAxisSize: props[mainName] == 'fit'
            ? MainAxisSize.min
            : MainAxisSize.max,
        mainAxisAlignment: switch (align) {
          'center' => MainAxisAlignment.center,
          'end' => MainAxisAlignment.end,
          'space_between' => MainAxisAlignment.spaceBetween,
          'space_around' => MainAxisAlignment.spaceAround,
          _ => MainAxisAlignment.start,
        },
        crossAxisAlignment: switch (cross) {
          'center' => CrossAxisAlignment.center,
          'end' => CrossAxisAlignment.end,
          _ => CrossAxisAlignment.start,
        },
        children: children,
      );
      return stretch ? _StretchCross(axis: crossAxis, child: flex) : flex;
    },
  );
}

/// A [Flex] whose children's flex is set aside while its main axis has no
/// end, where Flutter's would fail (they then take their own size).
class _BlockFlex extends Flex {
  const _BlockFlex({
    required super.direction,
    super.mainAxisAlignment,
    super.mainAxisSize,
    super.crossAxisAlignment,
    super.spacing,
    super.children,
  });

  @override
  RenderFlex createRenderObject(BuildContext context) => _RenderBlockFlex(
    direction: direction,
    mainAxisAlignment: mainAxisAlignment,
    mainAxisSize: mainAxisSize,
    crossAxisAlignment: crossAxisAlignment,
    textDirection: getEffectiveTextDirection(context),
    verticalDirection: verticalDirection,
    textBaseline: textBaseline,
    clipBehavior: clipBehavior,
    spacing: spacing,
  );
}

class _RenderBlockFlex extends RenderFlex {
  _RenderBlockFlex({
    super.direction,
    super.mainAxisAlignment,
    super.mainAxisSize,
    super.crossAxisAlignment,
    super.textDirection,
    super.verticalDirection,
    super.textBaseline,
    super.clipBehavior,
    super.spacing,
  });

  T _flexAsideIfEndless<T>(BoxConstraints constraints, T Function() layout) {
    final endless = switch (direction) {
      Axis.horizontal => !constraints.hasBoundedWidth,
      Axis.vertical => !constraints.hasBoundedHeight,
    };
    if (!endless) return layout();
    final aside = <FlexParentData, int>{};
    for (var child = firstChild; child != null; child = childAfter(child)) {
      final data = child.parentData! as FlexParentData;
      if (data.flex case final flex? when flex != 0) {
        aside[data] = flex;
        data.flex = null;
      }
    }
    try {
      return layout();
    } finally {
      aside.forEach((data, flex) => data.flex = flex);
    }
  }

  @override
  void performLayout() => _flexAsideIfEndless(constraints, super.performLayout);

  @override
  Size computeDryLayout(BoxConstraints constraints) => _flexAsideIfEndless(
    constraints,
    () => super.computeDryLayout(constraints),
  );

  @override
  double? computeDryBaseline(
    BoxConstraints constraints,
    TextBaseline baseline,
  ) => _flexAsideIfEndless(
    constraints,
    () => super.computeDryBaseline(constraints, baseline),
  );
}

/// `cross = "stretch"`: the children that set no size on [axis] fill it.
/// The room is what the parent gives, or, where that has no end (a row
/// inside a column), the tallest child's (as CSS's flex lines do).
class _StretchCross extends SingleChildRenderObjectWidget {
  const _StretchCross({required this.axis, super.child});

  final Axis axis;

  @override
  _RenderStretchCross createRenderObject(BuildContext context) =>
      _RenderStretchCross(axis);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderStretchCross renderObject,
  ) => renderObject.axis = axis;
}

class _RenderStretchCross extends RenderProxyBox {
  _RenderStretchCross(this._axis);

  Axis _axis;
  set axis(Axis value) {
    if (value == _axis) return;
    _axis = value;
    markNeedsLayout();
  }

  BoxConstraints _inner(
    BoxConstraints outer,
    RenderBox child,
  ) => switch (_axis) {
    Axis.vertical => outer.tighten(
      height: outer.hasBoundedHeight
          ? outer.maxHeight
          : outer.constrainHeight(child.getMaxIntrinsicHeight(outer.maxWidth)),
    ),
    Axis.horizontal => outer.tighten(
      width: outer.hasBoundedWidth
          ? outer.maxWidth
          : outer.constrainWidth(child.getMaxIntrinsicWidth(outer.maxHeight)),
    ),
  };

  @override
  void performLayout() {
    final child = this.child;
    if (child == null) {
      size = constraints.smallest;
      return;
    }
    child.layout(_inner(constraints, child), parentUsesSize: true);
    size = constraints.constrain(child.size);
  }

  @override
  Size computeDryLayout(BoxConstraints constraints) {
    final child = this.child;
    if (child == null) return constraints.smallest;
    return constraints.constrain(
      child.getDryLayout(_inner(constraints, child)),
    );
  }
}
