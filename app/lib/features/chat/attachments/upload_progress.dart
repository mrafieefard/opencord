import 'package:flutter/material.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/features/chat/attachments/file_icon.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// Shows the progress rows of outgoing uploads in a pending message bubble.
class UploadProgressList extends StatelessWidget {
  const UploadProgressList({super.key, required this.uploads, this.onCancel});

  final List<PendingUpload> uploads;
  final VoidCallback? onCancel;

  @override
  Widget build(BuildContext context) {
    if (uploads.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: OcSpace.s6),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final upload in uploads)
            _UploadRow(upload: upload, onCancel: onCancel),
        ],
      ),
    );
  }
}

class _UploadRow extends StatelessWidget {
  const _UploadRow({required this.upload, this.onCancel});

  final PendingUpload upload;
  final VoidCallback? onCancel;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final isProcessing = upload.phase == UploadPhase.processing;
    final isDone = upload.phase == UploadPhase.done;
    final failure = upload.phase == UploadPhase.failed
        ? upload.error ?? "Couldn't upload"
        : null;
    final progress = upload.size > 0
        ? (upload.sent / upload.size).clamp(0.0, 1.0)
        : 0.0;

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Icon(
                fileIcon(upload.name),
                size: 16,
                color: colors.textSecondary,
              ),
              const SizedBox(width: OcSpace.s6),
              Expanded(
                child: Text(
                  middleEllipsis(upload.name, maxLength: 28),
                  style: OcText.body.copyWith(color: colors.text),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              const SizedBox(width: OcSpace.s6),
              Text(
                isProcessing
                    ? 'Processing…'
                    : isDone || failure != null
                    ? formatFileSize(upload.size)
                    : '${formatFileSize(upload.sent)} / ${formatFileSize(upload.size)}',
                style: OcText.meta.copyWith(color: colors.textSecondary),
              ),
              if (onCancel != null) ...[
                const SizedBox(width: OcSpace.s4),
                Tooltip(
                  message: 'Cancel upload',
                  child: Hoverable(
                    onTap: onCancel,
                    semanticLabel: 'Cancel upload',
                    focusRadius: BorderRadius.circular(4),
                    builder: (context, state) => Icon(
                      OcIcons.close,
                      size: 14,
                      color: state.hovered ? colors.text : colors.textSecondary,
                    ),
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 4),
          if (failure != null)
            Text(
              failure,
              style: OcText.small.copyWith(color: colors.textSecondary),
            )
          else
            ClipRRect(
              borderRadius: BorderRadius.circular(2),
              child: SizedBox(
                height: 4,
                child: isProcessing
                    ? LinearProgressIndicator(
                        backgroundColor: colors.border,
                        valueColor: AlwaysStoppedAnimation<Color>(colors.text),
                      )
                    : LinearProgressIndicator(
                        value: progress,
                        backgroundColor: colors.border,
                        valueColor: AlwaysStoppedAnimation<Color>(colors.text),
                      ),
              ),
            ),
        ],
      ),
    );
  }
}
