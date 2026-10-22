import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// An error shown in place: text with an outlined error icon, never red
/// (§4.11).
class InlineError extends StatelessWidget {
  const InlineError(this.message, {super.key});

  final String message;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Semantics(
      liveRegion: true,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 1),
            child: Icon(OcIcons.error, size: 16, fill: 0, color: colors.text),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              message,
              style: OcText.small.copyWith(color: colors.text),
            ),
          ),
        ],
      ),
    );
  }
}
