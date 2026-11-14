import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `spacer` (design §12.2): fixed space when [size] is set; shares the free
/// room in a row or column when absent.
final spacerBlock = BlockRenderer(_spacer);

Widget _spacer(BlockBuild b) {
  final size = numberProp(b.props['size']);
  if (size != null) return SizedBox.square(dimension: size);
  return const SizedBox.shrink();
}

/// `divider` (design §12.2): a thin line of [thickness] and [color].
final dividerBlock = BlockRenderer(_divider);

Widget _divider(BlockBuild b) {
  final thickness = numberProp(b.props['thickness']) ?? 1.0;
  final color =
      b.theme.resolveColor(b.props['color']) ?? b.theme.colors['border']!;

  return LayoutBuilder(
    builder: (context, constraints) {
      if (constraints.maxHeight.isFinite && !constraints.maxWidth.isFinite) {
        return SizedBox(
          width: thickness,
          height: double.infinity,
          child: ColoredBox(color: color),
        );
      }
      return SizedBox(
        height: thickness,
        width: double.infinity,
        child: ColoredBox(color: color),
      );
    },
  );
}
