import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

Future<void> showTextPreviewDialog(
  BuildContext context, {
  required String filename,
  required Uint8List bytes,
}) {
  return showOcDialog(
    context: context,
    builder: (context) => TextPreviewDialog(filename: filename, bytes: bytes),
  );
}

class TextPreviewDialog extends StatelessWidget {
  const TextPreviewDialog({
    super.key,
    required this.filename,
    required this.bytes,
  });

  final String filename;
  final Uint8List bytes;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final text = utf8.decode(bytes, allowMalformed: true);

    return OcDialog(
      title: filename,
      width: 900,
      showClose: true,
      actions: [
        OcButton(label: 'Close', onPressed: () => Navigator.of(context).pop()),
      ],
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 900, maxHeight: 520),
        child: Container(
          width: double.infinity,
          padding: const EdgeInsets.all(OcSpace.s12),
          decoration: BoxDecoration(
            color: colors.surface,
            borderRadius: BorderRadius.circular(OcRadius.bubble),
            border: Border.all(color: colors.border),
          ),
          child: SingleChildScrollView(
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: SelectableText(
                text,
                style: OcText.mono.copyWith(color: colors.text),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
