import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// Opens a dialog over a scrim with a short fade and scale (§2.4). Escape
/// and a click on the scrim close it unless [dismissible] is false.
Future<T?> showOcDialog<T>({
  required BuildContext context,
  required WidgetBuilder builder,
  bool dismissible = true,
}) {
  final colors = context.oc;
  return showGeneralDialog<T>(
    context: context,
    barrierDismissible: dismissible,
    barrierLabel: 'Close',
    barrierColor: colors.scrim,
    transitionDuration: OcMotion.of(context).dialog,
    pageBuilder: (context, animation, secondaryAnimation) =>
        Builder(builder: builder),
    transitionBuilder: (context, animation, secondaryAnimation, child) {
      final curved = CurvedAnimation(parent: animation, curve: Curves.easeOut);
      return FadeTransition(
        opacity: curved,
        child: ScaleTransition(
          scale: Tween<double>(begin: 0.98, end: 1).animate(curved),
          child: child,
        ),
      );
    },
  );
}

/// The shared dialog shell (§4.11): title, content, and actions at the
/// bottom right (secondary first, then primary).
class OcDialog extends StatelessWidget {
  const OcDialog({
    super.key,
    required this.title,
    required this.child,
    this.actions = const [],
    this.width = 440,
    this.showClose = false,
  });

  final String title;
  final Widget child;
  final List<Widget> actions;
  final double width;

  /// A close button in the title row, for dialogs without a Cancel action.
  final bool showClose;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(OcSpace.s24),
        child: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: width),
          child: Material(
            type: MaterialType.transparency,
            child: Container(
              padding: const EdgeInsets.all(OcSpace.s24),
              decoration: BoxDecoration(
                color: colors.elevated,
                borderRadius: BorderRadius.circular(OcRadius.dialog),
                border: Border.all(color: colors.border),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Semantics(
                          header: true,
                          child: Text(
                            title,
                            style: OcText.title.copyWith(color: colors.text),
                          ),
                        ),
                      ),
                      if (showClose)
                        OcButton.ghost(
                          label: 'Close',
                          dense: true,
                          onPressed: () => Navigator.maybePop(context),
                        ),
                    ],
                  ),
                  const SizedBox(height: OcSpace.s16),
                  DefaultTextStyle(
                    style: OcText.body.copyWith(color: colors.textSecondary),
                    child: child,
                  ),
                  if (actions.isNotEmpty) ...[
                    const SizedBox(height: OcSpace.s24),
                    Wrap(
                      alignment: WrapAlignment.end,
                      spacing: OcSpace.s8,
                      runSpacing: OcSpace.s8,
                      children: actions,
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
