import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `stack` (design §12.2): layers of children; children may set `x`, `y`,
/// `right` and `bottom` to be positioned; unpositioned children fill the stack.
final stackBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  final controller = b.controller;
  final ids = b.node.children;

  return ListenableBuilder(
    listenable: Listenable.merge([for (final id in ids) controller.watch(id)]),
    builder: (context, _) {
      final children = <Widget>[];
      for (final id in ids) {
        final child = controller.node(id);
        if (child == null || child.props['visible'] == false) continue;

        final props = child.props;
        final x = numberProp(props['x']);
        final y = numberProp(props['y']);
        final right = numberProp(props['right']);
        final bottom = numberProp(props['bottom']);

        final isPositioned =
            x != null || y != null || right != null || bottom != null;
        children.add(
          isPositioned
              ? Positioned(
                  key: ValueKey<int>(id),
                  left: x,
                  top: y,
                  right: right,
                  bottom: bottom,
                  child: BlockWidget(id: id),
                )
              : BlockWidget(key: ValueKey<int>(id), id: id),
        );
      }
      return LayoutBuilder(
        builder: (context, constraints) {
          final hasBoundedWidth = constraints.hasBoundedWidth;
          final hasBoundedHeight = constraints.hasBoundedHeight;
          final fit = (hasBoundedWidth && hasBoundedHeight)
              ? StackFit.expand
              : StackFit.loose;
          return Stack(fit: fit, clipBehavior: Clip.none, children: children);
        },
      );
    },
  );
}
