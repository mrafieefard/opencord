import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/music/music_access.dart';
import 'package:opencord/features/music/music_uploads.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Song files dragged onto the music player (design 2026-10-24): queued
/// last, or played at once when dropped on the cover below it (the one
/// that marks itself with [coverKeyOf]).
class MusicDropZone extends ConsumerStatefulWidget {
  const MusicDropZone({super.key, required this.channel, required this.child});

  final MusicChannel channel;
  final Widget child;

  /// The key the cover inside a drop zone puts on itself.
  static GlobalKey? coverKeyOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_CoverKey>()?.cover;

  @override
  ConsumerState<MusicDropZone> createState() => _MusicDropZoneState();
}

class _CoverKey extends InheritedWidget {
  const _CoverKey({required this.cover, required super.child});

  final GlobalKey cover;

  @override
  bool updateShouldNotify(_CoverKey old) => old.cover != cover;
}

class _MusicDropZoneState extends ConsumerState<MusicDropZone> {
  var _dragging = false;
  final _cover = GlobalKey(debugLabel: 'music cover');

  bool _onCover(Offset global) {
    final box = _cover.currentContext?.findRenderObject();
    if (box is! RenderBox || !box.hasSize) return false;
    return (box.localToGlobal(Offset.zero) & box.size).contains(global);
  }

  void _drop(DropDoneDetails details, MusicAccess access) {
    setState(() => _dragging = false);
    if (!access.canAdd) return;
    final playNow = access.canManageQueue && _onCover(details.globalPosition);
    final refused = ref
        .read(musicUploadsProvider(widget.channel).notifier)
        .drop([for (final file in details.files) file.path], playNow: playNow);
    if (refused.isNotEmpty) {
      showOcToast(
        context,
        refused.length == 1
            ? '${refused.single.split(RegExp(r'[/\\]')).last} isn’t a song '
                  'file the music player takes'
            : '${refused.length} files aren’t songs the music player takes',
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final access = ref.watch(musicAccessProvider(widget.channel));
    return DropTarget(
      onDragEntered: (_) => setState(() => _dragging = true),
      onDragExited: (_) => setState(() => _dragging = false),
      onDragDone: (details) => _drop(details, access),
      child: Stack(
        fit: StackFit.passthrough,
        children: [
          _CoverKey(cover: _cover, child: widget.child),
          if (_dragging)
            Positioned.fill(
              child: IgnorePointer(
                child: ColoredBox(
                  color: colors.scrim,
                  child: Center(
                    child: Container(
                      margin: const EdgeInsets.all(OcSpace.s16),
                      padding: const EdgeInsets.all(OcSpace.s20),
                      decoration: BoxDecoration(
                        color: colors.elevated,
                        borderRadius: BorderRadius.circular(OcRadius.dialog),
                        border: Border.all(color: colors.text, width: 1.5),
                      ),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(
                            OcIcons.libraryMusic,
                            size: 32,
                            color: colors.text,
                          ),
                          const SizedBox(height: OcSpace.s12),
                          Text(
                            access.canAdd
                                ? 'Drop songs to add them to the queue'
                                : access.why(Permissions.musicAdd) ??
                                      'You can’t add music here',
                            textAlign: TextAlign.center,
                            style: OcText.title.copyWith(color: colors.text),
                          ),
                          if (access.canAdd && access.canManageQueue) ...[
                            const SizedBox(height: OcSpace.s6),
                            Text(
                              'Drop on the cover to play now',
                              textAlign: TextAlign.center,
                              style: OcText.small.copyWith(
                                color: colors.textSecondary,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// The channel's dropped songs on their way into the queue: each with its
/// progress and a cancel button, or why it failed.
class MusicUploadsList extends ConsumerWidget {
  const MusicUploadsList({super.key, required this.channel});

  final MusicChannel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final uploads = ref.watch(musicUploadsProvider(channel));
    return SliverList.builder(
      itemCount: uploads.length,
      itemBuilder: (context, index) =>
          MusicUploadRow(channel: channel, upload: uploads[index]),
    );
  }
}

class MusicUploadRow extends ConsumerWidget {
  const MusicUploadRow({
    super.key,
    required this.channel,
    required this.upload,
  });

  final MusicChannel channel;
  final MusicUpload upload;

  String get _status => switch (upload.phase) {
    UploadPhase.preparing => 'Checking…',
    UploadPhase.uploading =>
      '${formatFileSize(upload.sentBytes)} of ${formatFileSize(upload.totalBytes)}',
    UploadPhase.processing => 'The server is checking it…',
    UploadPhase.failed => upload.error ?? 'It couldn’t be added',
    _ => '',
  };

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final notifier = ref.read(musicUploadsProvider(channel).notifier);
    final uploading = upload.phase == UploadPhase.uploading;
    return Semantics(
      label: '${upload.filename}, $_status',
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: OcSpace.s4),
        child: Row(
          children: [
            SizedBox(
              width: 40,
              height: 40,
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: colors.hover,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Icon(
                  upload.failed ? OcIcons.error : OcIcons.upload,
                  size: 20,
                  color: upload.failed ? colors.text : colors.textSecondary,
                ),
              ),
            ),
            const SizedBox(width: OcSpace.s10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    upload.filename,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.body.copyWith(color: colors.text),
                  ),
                  const SizedBox(height: OcSpace.s2),
                  if (uploading)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: OcSpace.s4),
                      child: ClipRRect(
                        borderRadius: BorderRadius.circular(2),
                        child: LinearProgressIndicator(
                          value: upload.progress,
                          minHeight: 4,
                          color: colors.accent,
                          backgroundColor: colors.hover,
                        ),
                      ),
                    ),
                  Text(
                    _status,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.small.copyWith(
                      color: upload.failed ? colors.text : colors.textMuted,
                    ),
                  ),
                ],
              ),
            ),
            OcIconButton(
              icon: OcIcons.close,
              tooltip: upload.failed ? 'Dismiss' : 'Cancel upload',
              size: OcIconButtonSize.compact,
              onPressed: () => upload.failed
                  ? notifier.dismiss(upload.id)
                  : notifier.cancel(upload.id),
            ),
          ],
        ),
      ),
    );
  }
}

/// Removes an uploaded song from the server, after asking: the caller's
/// own, or anyone's for an administrator. True when it went.
Future<bool> removeLocalSong(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  MusicTrack track,
) async {
  final admin = ref
      .read(serverProvider(serverKey))
      .data
      ?.can(Permissions.administrator);
  final confirmed = await confirmAction(
    context,
    title: 'Remove ‘${track.title}’?',
    message: admin ?? false
        ? 'It is removed from the server for everyone. Queues that hold '
              'it skip it.'
        : 'It leaves your uploaded songs. Others who uploaded the same '
              'file keep it.',
    action: 'Remove from server',
  );
  if (!confirmed || !context.mounted) return false;
  try {
    await ref
        .read(repositoryProvider)
        .removeLocalSong(serverKey, track.localSongId);
    return true;
  } on RepoException catch (error) {
    if (context.mounted) showOcToast(context, error.message);
    return false;
  }
}
