import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// An upper-case section heading like "TEXT CHANNELS" or "ONLINE — 5".
class SectionLabel extends StatelessWidget {
  const SectionLabel(
    this.text, {
    super.key,
    this.trailing,
    this.padding = EdgeInsets.zero,
  });

  final String text;
  final Widget? trailing;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: padding,
      child: Row(
        children: [
          Expanded(
            child: Semantics(
              header: true,
              child: Text(
                text.toUpperCase(),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.label.copyWith(color: colors.textMuted),
              ),
            ),
          ),
          ?trailing,
        ],
      ),
    );
  }
}
