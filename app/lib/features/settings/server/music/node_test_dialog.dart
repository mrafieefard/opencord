import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';

/// A node's connection test (Phase 3 plan §12.9): three steps, each with
/// ✓ or ✕ and what to do when it failed. It starts when the dialog opens.
class LavalinkTestDialog extends ConsumerStatefulWidget {
  const LavalinkTestDialog({
    super.key,
    required this.serverKey,
    required this.node,
  });

  final String serverKey;
  final LavalinkNode node;

  @override
  ConsumerState<LavalinkTestDialog> createState() => _TestDialogState();
}

class _TestDialogState extends ConsumerState<LavalinkTestDialog> {
  @override
  void initState() {
    super.initState();
    // Not while the tree builds: the test's state is a provider.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _start();
    });
  }

  void _start() => ref
      .read(lavalinkTestsProvider(widget.serverKey).notifier)
      .start(widget.node.id);

  @override
  Widget build(BuildContext context) {
    final progress =
        ref.watch(
          lavalinkTestsProvider(
            widget.serverKey,
          ).select((tests) => tests[widget.node.id]),
        ) ??
        LavalinkTestProgress.started(widget.node.id);
    return OcDialog(
      title: 'Test ${widget.node.settings.name}',
      width: 520,
      actions: [
        OcButton(label: 'Run again', onPressed: progress.done ? _start : null),
        OcButton.primary(
          label: 'Close',
          onPressed: () => Navigator.pop(context),
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final (index, step) in progress.steps.indexed) ...[
            if (index > 0) const SizedBox(height: OcSpace.s12),
            _StepRow(step: step),
          ],
          if (progress.passed) ...[
            const SizedBox(height: OcSpace.s16),
            Text(
              'This node is ready to play music.',
              style: OcText.bodyStrong.copyWith(color: context.oc.text),
            ),
          ],
        ],
      ),
    );
  }
}

class _StepRow extends StatelessWidget {
  const _StepRow({required this.step});

  final LavalinkTestStep step;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final (icon, label) = switch (step.state) {
      LavalinkTestStepState.passed => (OcIcons.check, 'Passed'),
      LavalinkTestStepState.failed => (OcIcons.close, 'Failed'),
      LavalinkTestStepState.skipped => (OcIcons.remove, 'Not run'),
      LavalinkTestStepState.waiting => (OcIcons.hourglassEmpty, 'Waiting'),
      LavalinkTestStepState.running => (null, 'Running'),
    };
    final dimmed =
        step.state == LavalinkTestStepState.skipped ||
        step.state == LavalinkTestStepState.waiting;
    return Semantics(
      container: true,
      label: '${step.kind.label}: $label',
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          ExcludeSemantics(
            child: SizedBox(
              width: OcSize.iconRow,
              height: OcSize.iconRow,
              child: switch (icon) {
                final icon? => Icon(
                  icon,
                  size: OcSize.iconRow,
                  color: dimmed ? colors.textMuted : colors.text,
                ),
                null => const OcSpinner(),
              },
            ),
          ),
          const SizedBox(width: OcSpace.s12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                ExcludeSemantics(
                  child: Text(
                    step.kind.label,
                    style: OcText.bodyStrong.copyWith(
                      color: dimmed ? colors.textMuted : colors.text,
                    ),
                  ),
                ),
                if (step.detail.isNotEmpty)
                  SelectableText(
                    step.detail,
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
