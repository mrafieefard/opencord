import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/message_actions.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

final downloadProgressProvider =
    NotifierProvider<DownloadProgressNotifier, Map<int, DownloadProgressEvent>>(
      DownloadProgressNotifier.new,
    );

class DownloadProgressNotifier
    extends Notifier<Map<int, DownloadProgressEvent>> {
  @override
  Map<int, DownloadProgressEvent> build() {
    try {
      final sub = ref.watch(repositoryProvider).events.listen((event) {
        if (event is DownloadProgressEvent) {
          final next = Map<int, DownloadProgressEvent>.from(state);
          if (event.done || event.error != null) {
            next.remove(event.attachmentId);
          } else {
            next[event.attachmentId] = event;
          }
          state = next;
        }
      });
      ref.onDispose(sub.cancel);
    } catch (_) {
      // In tests where repositoryProvider is not overridden.
    }
    return const {};
  }
}

Future<void> saveAttachment(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  Attachment attachment,
) async {
  final location = await getSaveLocation(suggestedName: attachment.filename);
  if (location == null) return;
  try {
    await ref
        .read(repositoryProvider)
        .downloadAttachment(serverKey, attachment.id, location.path);
    if (context.mounted) {
      showOcToast(context, 'Saved ${attachment.filename}');
    }
  } catch (error) {
    if (error is RepoException && error.kind == RepoErrorKind.cancelled) {
      return;
    }
    if (context.mounted) {
      final message = error is RepoException ? error.message : error.toString();
      showOcToast(context, message);
    }
  }
}

Future<void> showAttachmentMenu({
  required BuildContext context,
  required WidgetRef ref,
  required Offset position,
  required String serverKey,
  required Attachment attachment,
  Message? message,
}) async {
  MessageActions? actions;
  if (message != null) {
    actions = MessageActions(
      context: context,
      ref: ref,
      channel: (server: serverKey, channel: message.channelId),
      controller: ChatController(),
    );
  }
  final isImage = attachment.kind == AttachmentKind.image;
  final entries = <OcMenuEntry>[
    OcMenuItem(
      label: 'Download',
      icon: OcIcons.download,
      onSelected: () => saveAttachment(context, ref, serverKey, attachment),
    ),
    if (isImage)
      OcMenuItem(
        label: 'Copy image',
        icon: OcIcons.contentCopy,
        onSelected: () async {
          try {
            final bytes = await ref.read(
              attachmentBytesProvider((
                server: serverKey,
                id: attachment.id,
                thumbnail: false,
              )).future,
            );
            await ref.read(repositoryProvider).copyImage(bytes);
            if (context.mounted) {
              showOcToast(context, 'Copied');
            }
          } catch (error) {
            if (context.mounted) {
              final message = error is RepoException
                  ? error.message
                  : error.toString();
              showOcToast(context, message);
            }
          }
        },
      ),
    if (actions != null && message != null && actions.canDelete(message)) ...[
      const OcMenuDivider(),
      OcMenuItem(
        label: 'Delete',
        icon: OcIcons.delete,
        onSelected: () => actions!.delete(message),
      ),
    ],
  ];
  await showOcMenu(context: context, position: position, entries: entries);
}
