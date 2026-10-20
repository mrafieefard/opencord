import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/repository/repository.dart';
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

/// Saves each of [attachments] into a folder the person picks, beside
/// what is there already rather than over it.
Future<void> saveAttachmentsToFolder(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  List<Attachment> attachments,
) async {
  final directory = await getDirectoryPath(confirmButtonText: 'Save here');
  if (directory == null) return;
  final repository = ref.read(repositoryProvider);
  final taken = <String>{};
  var saved = 0;
  try {
    for (final attachment in attachments) {
      final path = freePathIn(
        directory,
        attachment.filename,
        exists: (path) => taken.contains(path) || File(path).existsSync(),
      );
      taken.add(path);
      await repository.downloadAttachment(serverKey, attachment.id, path);
      saved++;
    }
    if (context.mounted) {
      showOcToast(context, saved == 1 ? 'Saved 1 file' : 'Saved $saved files');
    }
  } on RepoException catch (error) {
    if (error.kind == RepoErrorKind.cancelled || !context.mounted) return;
    showOcToast(context, error.message);
  }
}

/// [name] made safe to save as: no folders, no control characters.
String safeFileName(String name) {
  final base = name.split(RegExp(r'[/\\]')).last;
  final clean = base.replaceAll(RegExp(r'[\x00-\x1f\x7f]'), '').trim();
  return switch (clean) {
    '' || '.' || '..' => 'file',
    _ => clean,
  };
}

/// A path in [directory] for [name] that nothing [exists] at: `name (2).ext`
/// and so on when it is taken.
String freePathIn(
  String directory,
  String name, {
  required bool Function(String path) exists,
}) {
  final safe = safeFileName(name);
  final dot = safe.lastIndexOf('.');
  final (stem, extension) = dot > 0
      ? (safe.substring(0, dot), safe.substring(dot))
      : (safe, '');
  final separator = Platform.pathSeparator;
  final folder = directory.endsWith(separator)
      ? directory
      : '$directory$separator';
  var path = '$folder$safe';
  for (var n = 2; exists(path); n++) {
    path = '$folder$stem ($n)$extension';
  }
  return path;
}

Future<void> copyAttachmentImage(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  Attachment attachment,
) async {
  final repository = ref.read(repositoryProvider);
  try {
    final bytes = await repository.attachmentBytes(serverKey, attachment.id);
    await repository.copyImage(bytes);
    if (context.mounted) showOcToast(context, 'Copied');
  } on RepoException catch (error) {
    if (context.mounted) showOcToast(context, error.message);
  }
}

/// Marks an attachment's tile, so a right-click on it can tell which one
/// the message's menu is about.
class AttachmentTarget extends StatelessWidget {
  const AttachmentTarget({
    super.key,
    required this.attachment,
    required this.child,
  });

  final Attachment attachment;
  final Widget child;

  @override
  Widget build(BuildContext context) => MetaData(
    metaData: attachment,
    behavior: HitTestBehavior.opaque,
    child: child,
  );
}
