import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/message_actions.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

final downloadProgressProvider =
    NotifierProvider<DownloadProgressNotifier, Map<int, DownloadProgressEvent>>(
      DownloadProgressNotifier.new,
    );

/// Downloads in progress, by attachment; each leaves on its last event
/// (`done`, or an error such as a cancel).
class DownloadProgressNotifier
    extends Notifier<Map<int, DownloadProgressEvent>> {
  @override
  Map<int, DownloadProgressEvent> build() {
    final changes = ref.watch(repositoryProvider).events.listen((event) {
      if (event is! DownloadProgressEvent) return;
      final next = Map.of(state);
      if (event.done || event.error != null) {
        next.remove(event.attachmentId);
      } else {
        next[event.attachmentId] = event;
      }
      state = next;
    });
    ref.onDispose(changes.cancel);
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
  } on RepoException catch (error) {
    if (error.kind == RepoErrorKind.cancelled || !context.mounted) return;
    showOcToast(context, error.message);
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
          final repository = ref.read(repositoryProvider);
          try {
            final bytes = await repository.attachmentBytes(
              serverKey,
              attachment.id,
            );
            await repository.copyImage(bytes);
            if (context.mounted) showOcToast(context, 'Copied');
          } on RepoException catch (error) {
            if (context.mounted) showOcToast(context, error.message);
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
