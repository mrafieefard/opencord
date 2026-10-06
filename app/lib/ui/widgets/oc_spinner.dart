import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';

/// A small progress ring for buttons and connecting states. Lists never use
/// spinners; they show skeleton rows (§4.13).
class OcSpinner extends StatelessWidget {
  const OcSpinner({super.key, this.size = 16, this.color});

  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: CircularProgressIndicator(
        strokeWidth: 2,
        color: color ?? context.oc.text,
      ),
    );
  }
}
