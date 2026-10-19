import 'package:flutter/material.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/upload_rules.dart';
import 'package:opencord/features/chat/attachments/file_icon.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// The horizontal list of staged files above the composer text field.
class StagedTiles extends StatelessWidget {
  const StagedTiles({
    super.key,
    required this.files,
    required this.limit,
    required this.blocked,
    required this.maxFiles,
    required this.onRemove,
  });

  final List<StagedFile> files;
  final int limit;

  /// The server's blocked extensions, to name the one a file matched.
  final List<String> blocked;
  final int maxFiles;
  final ValueChanged<String> onRemove;

  @override
  Widget build(BuildContext context) {
    if (files.isEmpty) return const SizedBox.shrink();
    return SizedBox(
      height: 72,
      child: ListView.separated(
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: 4,
        ),
        scrollDirection: Axis.horizontal,
        itemCount: files.length,
        separatorBuilder: (_, _) => const SizedBox(width: OcSpace.s8),
        itemBuilder: (context, index) => _StagedTile(
          file: files[index],
          limit: limit,
          blocked: blocked,
          maxFiles: maxFiles,
          onRemove: () => onRemove(files[index].id),
        ),
      ),
    );
  }
}

class _StagedTile extends StatelessWidget {
  const _StagedTile({
    required this.file,
    required this.limit,
    required this.blocked,
    required this.maxFiles,
    required this.onRemove,
  });

  final StagedFile file;
  final int limit;
  final List<String> blocked;
  final int maxFiles;
  final VoidCallback onRemove;

  String _reasonText(StageProblem problem) => switch (problem) {
    StageProblem.tooLarge => 'Larger than your ${formatFileSize(limit)} limit',
    StageProblem.blocked =>
      '${blockedExtension(sanitizeFilename(file.name), blocked) ?? fileExtension(file.name)} '
          'files are blocked here',
    StageProblem.tooMany => 'Only $maxFiles files per message',
  };

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final problem = file.problem;
    final hasProblem = problem != null;

    return Container(
      width: 200,
      height: 64,
      padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8, vertical: 6),
      decoration: BoxDecoration(
        color: colors.surface,
        borderRadius: BorderRadius.circular(OcRadius.bubble),
        border: Border.all(
          color: hasProblem ? colors.text : colors.border,
          width: 1.0,
        ),
      ),
      child: Row(
        children: [
          // Left: preview thumbnail, icon, or problem icon
          if (hasProblem)
            Icon(OcIcons.error, size: 28, color: colors.text)
          else if (file.preview != null)
            SizedBox(
              width: 44,
              height: 44,
              child: ClipRRect(
                borderRadius: BorderRadius.circular(4),
                child: Image.memory(
                  file.preview!,
                  width: 44,
                  height: 44,
                  fit: BoxFit.cover,
                  errorBuilder: (_, _, _) => Container(
                    width: 44,
                    height: 44,
                    decoration: BoxDecoration(
                      color: colors.elevated,
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Icon(
                      fileIcon(file.name),
                      size: 22,
                      color: colors.textSecondary,
                    ),
                  ),
                ),
              ),
            )
          else
            Container(
              width: 44,
              height: 44,
              decoration: BoxDecoration(
                color: colors.elevated,
                borderRadius: BorderRadius.circular(4),
              ),
              child: Icon(
                fileIcon(file.name),
                size: 22,
                color: colors.textSecondary,
              ),
            ),
          const SizedBox(width: OcSpace.s8),
          // Middle: filename and size or problem reason
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(
                  middleEllipsis(file.name, maxLength: 16),
                  style: OcText.bodyStrong.copyWith(color: colors.text),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 2),
                Text(
                  hasProblem ? _reasonText(problem) : formatFileSize(file.size),
                  style: OcText.small.copyWith(color: colors.textSecondary),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ],
            ),
          ),
          // Right: X remove button
          Tooltip(
            message: 'Remove ${file.name}',
            child: Hoverable(
              onTap: onRemove,
              semanticLabel: 'Remove ${file.name}',
              focusRadius: BorderRadius.circular(4),
              builder: (context, state) => Icon(
                OcIcons.close,
                size: 16,
                color: state.hovered ? colors.text : colors.textSecondary,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
