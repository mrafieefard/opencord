import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/features/chat/attachments/text_preview_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

const textPreviewExtensions = {
  'txt',
  'md',
  'log',
  'json',
  'csv',
  'xml',
  'yaml',
  'yml',
  'toml',
  'ini',
  'cfg',
  'conf',
  'rs',
  'py',
  'js',
  'ts',
  'dart',
  'c',
  'h',
  'cpp',
  'java',
  'go',
  'sh',
};

const int maxPreviewBytes = 256 * 1024;

class FileCard extends ConsumerWidget {
  const FileCard({
    super.key,
    required this.serverKey,
    required this.attachment,
    this.message,
  });

  final String serverKey;
  final Attachment attachment;
  final Message? message;

  static IconData iconFor(Attachment attachment) {
    if (attachment.kind == AttachmentKind.image) return OcIcons.image;
    if (attachment.kind == AttachmentKind.video) return OcIcons.movie;
    if (attachment.kind == AttachmentKind.audio ||
        attachment.kind == AttachmentKind.voiceMessage) {
      return OcIcons.audioFile;
    }
    final dot = attachment.filename.lastIndexOf('.');
    final ext = dot != -1
        ? attachment.filename.substring(dot + 1).toLowerCase()
        : '';
    if (const {
      'zip',
      '7z',
      'rar',
      'tar',
      'gz',
      'bz2',
      'xz',
      'tgz',
    }.contains(ext)) {
      return OcIcons.folderZip;
    }
    if (textPreviewExtensions.contains(ext)) {
      return OcIcons.article;
    }
    if (const {
      'png',
      'jpg',
      'jpeg',
      'gif',
      'webp',
      'svg',
      'bmp',
      'ico',
    }.contains(ext)) {
      return OcIcons.image;
    }
    if (const {'mp4', 'webm', 'mkv', 'mov', 'avi'}.contains(ext)) {
      return OcIcons.movie;
    }
    if (const {'mp3', 'ogg', 'wav', 'flac', 'm4a', 'opus'}.contains(ext)) {
      return OcIcons.audioFile;
    }
    return OcIcons.description;
  }

  bool get _canPreview {
    final dot = attachment.filename.lastIndexOf('.');
    final ext = dot != -1
        ? attachment.filename.substring(dot + 1).toLowerCase()
        : '';
    return textPreviewExtensions.contains(ext) &&
        attachment.sizeBytes <= maxPreviewBytes;
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final download = ref.watch(downloadProgressProvider)[attachment.id];
    final isDownloading = download != null && !download.done;

    return GestureDetector(
      onSecondaryTapUp: (details) => showAttachmentMenu(
        context: context,
        ref: ref,
        position: details.globalPosition,
        serverKey: serverKey,
        attachment: attachment,
        message: message,
      ),
      child: Container(
        width: 320,
        height: 56,
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s8,
        ),
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: BorderRadius.circular(OcRadius.bubble),
          border: Border.all(color: colors.border),
        ),
        child: Row(
          children: [
            Icon(
              iconFor(attachment),
              size: OcSize.iconButton,
              color: colors.textSecondary,
            ),
            const SizedBox(width: OcSpace.s10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Text(
                    middleEllipsis(attachment.filename, maxLength: 22),
                    style: OcText.bodyStrong.copyWith(color: colors.text),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  const SizedBox(height: 2),
                  if (isDownloading)
                    ClipRRect(
                      borderRadius: BorderRadius.circular(2),
                      child: LinearProgressIndicator(
                        value: download.totalBytes > 0
                            ? download.receivedBytes / download.totalBytes
                            : null,
                        minHeight: 4,
                        backgroundColor: colors.border,
                        valueColor: AlwaysStoppedAnimation<Color>(
                          colors.accent,
                        ),
                      ),
                    )
                  else
                    Text(
                      formatFileSize(attachment.sizeBytes),
                      style: OcText.small.copyWith(color: colors.textSecondary),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                ],
              ),
            ),
            const SizedBox(width: OcSpace.s8),
            if (isDownloading)
              IconButton(
                icon: const Icon(OcIcons.close, size: OcSize.iconInline),
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                tooltip: 'Cancel',
                onPressed: () => ref
                    .read(repositoryProvider)
                    .cancelDownload(serverKey, attachment.id),
              )
            else ...[
              if (_canPreview) ...[
                OcButton.ghost(
                  label: 'Preview',
                  dense: true,
                  onPressed: () async {
                    try {
                      final bytes = await ref.read(
                        attachmentBytesProvider((
                          server: serverKey,
                          id: attachment.id,
                          thumbnail: false,
                        )).future,
                      );
                      if (context.mounted) {
                        showTextPreviewDialog(
                          context,
                          filename: attachment.filename,
                          bytes: bytes,
                        );
                      }
                    } catch (error) {
                      if (context.mounted) {
                        showOcToast(context, error.toString());
                      }
                    }
                  },
                ),
                const SizedBox(width: OcSpace.s4),
              ],
              OcButton.ghost(
                label: 'Download',
                dense: true,
                onPressed: () =>
                    saveAttachment(context, ref, serverKey, attachment),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
