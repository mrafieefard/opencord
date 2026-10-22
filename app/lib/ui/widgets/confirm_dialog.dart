import 'package:flutter/material.dart';

import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

/// The confirm dialog (§4.11): a title, one sentence, Cancel and an
/// explicit primary action such as "Delete channel". True when confirmed.
Future<bool> confirmAction(
  BuildContext context, {
  required String title,
  required String message,
  required String action,
}) async {
  final confirmed = await showOcDialog<bool>(
    context: context,
    builder: (context) => OcDialog(
      title: title,
      actions: [
        OcButton(
          label: 'Cancel',
          onPressed: () => Navigator.pop(context, false),
        ),
        OcButton.primary(
          label: action,
          autofocus: true,
          onPressed: () => Navigator.pop(context, true),
        ),
      ],
      child: Text(message),
    ),
  );
  return confirmed ?? false;
}
