import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// A 52 px panel header with a 1 px bottom border line.
///
/// Does not drag the window (§5.8).
class PanelHeader extends StatelessWidget {
  const PanelHeader({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.only(left: 16, right: 8),
    this.color,
  });

  final Widget child;
  final EdgeInsets padding;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      constraints: const BoxConstraints(minHeight: OcFloats.header),
      decoration: BoxDecoration(
        color: color ?? colors.sidebar,
        border: Border(bottom: BorderSide(color: colors.border)),
      ),
      padding: padding,
      alignment: Alignment.centerLeft,
      child: child,
    );
  }
}
