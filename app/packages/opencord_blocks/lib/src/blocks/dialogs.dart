import 'package:flutter/material.dart';

import '../controller.dart';
import '../theme.dart';

/// Draws quick dialogs (`confirm`, `alert`, `prompt`) on top of the view,
/// answering through [BlocksController.answerDialog].
class QuickDialogsLayer extends StatelessWidget {
  const QuickDialogsLayer({
    super.key,
    required this.controller,
    required this.theme,
  });

  final BlocksController controller;
  final BlocksTheme theme;

  @override
  Widget build(BuildContext context) {
    final dialogs = controller.dialogs;
    if (dialogs.isEmpty) return const SizedBox.shrink();

    return Stack(
      fit: StackFit.expand,
      children: [
        for (final dialog in dialogs)
          _QuickDialogWidget(
            key: ValueKey<int>(dialog.id),
            dialog: dialog,
            controller: controller,
            theme: theme,
          ),
      ],
    );
  }
}

class _QuickDialogWidget extends StatefulWidget {
  const _QuickDialogWidget({
    super.key,
    required this.dialog,
    required this.controller,
    required this.theme,
  });

  final BlocksDialog dialog;
  final BlocksController controller;
  final BlocksTheme theme;

  @override
  State<_QuickDialogWidget> createState() => _QuickDialogWidgetState();
}

class _QuickDialogWidgetState extends State<_QuickDialogWidget> {
  late final TextEditingController _textController;

  @override
  void initState() {
    super.initState();
    _textController = TextEditingController(
      text: widget.dialog.defaultText ?? '',
    );
  }

  @override
  void dispose() {
    _textController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final dialog = widget.dialog;
    final theme = widget.theme;

    Widget actions;
    if (dialog.kind == 'confirm') {
      actions = Row(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [
          _dialogButton(
            label: 'Cancel',
            primary: false,
            onPressed: () => widget.controller.answerDialog(dialog.id, false),
          ),
          const SizedBox(width: 8),
          _dialogButton(
            label: 'OK',
            primary: true,
            onPressed: () => widget.controller.answerDialog(dialog.id, true),
          ),
        ],
      );
    } else if (dialog.kind == 'prompt') {
      actions = Row(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [
          _dialogButton(
            label: 'Cancel',
            primary: false,
            onPressed: () => widget.controller.answerDialog(dialog.id, null),
          ),
          const SizedBox(width: 8),
          _dialogButton(
            label: 'OK',
            primary: true,
            onPressed: () =>
                widget.controller.answerDialog(dialog.id, _textController.text),
          ),
        ],
      );
    } else {
      // alert
      actions = Row(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [
          _dialogButton(
            label: 'OK',
            primary: true,
            onPressed: () => widget.controller.answerDialog(dialog.id, null),
          ),
        ],
      );
    }

    return Container(
      color: Colors.black54,
      alignment: Alignment.center,
      padding: const EdgeInsets.all(24),
      child: Material(
        type: MaterialType.transparency,
        child: Container(
          width: 320,
          padding: const EdgeInsets.all(20),
          decoration: BoxDecoration(
            color: theme.colors['elevated']!,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: theme.colors['border']!, width: 1),
            boxShadow: theme.shadows['md'],
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                dialog.text,
                style: TextStyle(
                  fontFamily: theme.text.fontFamily ?? 'Inter',
                  fontSize: 15,
                  fontWeight: FontWeight.w500,
                  color: theme.colors['text']!,
                ),
              ),
              if (dialog.kind == 'prompt') ...[
                const SizedBox(height: 12),
                TextField(
                  controller: _textController,
                  autofocus: true,
                  style: TextStyle(
                    fontFamily: theme.text.fontFamily ?? 'Inter',
                    fontSize: 14,
                    color: theme.colors['text']!,
                  ),
                  decoration: InputDecoration(
                    isDense: true,
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 10,
                    ),
                    fillColor: theme.colors['surface']!,
                    filled: true,
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(8),
                      borderSide: BorderSide(
                        color: theme.colors['border']!,
                        width: 1,
                      ),
                    ),
                  ),
                ),
              ],
              const SizedBox(height: 20),
              actions,
            ],
          ),
        ),
      ),
    );
  }

  Widget _dialogButton({
    required String label,
    required bool primary,
    required VoidCallback onPressed,
  }) {
    final theme = widget.theme;
    final bg = primary ? theme.colors['accent']! : theme.colors['surface']!;
    final fg = primary ? theme.colors['on_accent']! : theme.colors['text']!;
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        onTap: onPressed,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          decoration: BoxDecoration(
            color: bg,
            borderRadius: BorderRadius.circular(8),
            border: primary
                ? null
                : Border.all(color: theme.colors['border']!, width: 1),
          ),
          child: Text(
            label,
            style: TextStyle(
              fontFamily: theme.text.fontFamily ?? 'Inter',
              fontSize: 13,
              fontWeight: FontWeight.w600,
              color: fg,
            ),
          ),
        ),
      ),
    );
  }
}
