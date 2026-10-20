import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/providers/playback_state.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/features/chat/attachments/video_player_view.dart';
import 'package:opencord/features/window/window_buttons.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// Shows [media] from [index] as Telegram does: a see-through layer over
/// the whole window, title bar included, that appears and goes at once.
Future<void> showMediaViewer(
  BuildContext context, {
  required String serverKey,
  required List<Attachment> media,
  required int index,
}) {
  return Navigator.of(context, rootNavigator: true).push<void>(
    PageRouteBuilder<void>(
      opaque: false,
      transitionDuration: Duration.zero,
      reverseTransitionDuration: Duration.zero,
      // Dark in either theme: everything here sits on the dark scrim.
      pageBuilder: (context, animation, secondaryAnimation) => Theme(
        data: buildTheme(OcColors.dark),
        child: MediaViewer(
          serverKey: serverKey,
          media: media,
          initialIndex: index,
        ),
      ),
    ),
  );
}

class MediaViewer extends ConsumerStatefulWidget {
  const MediaViewer({
    super.key,
    required this.serverKey,
    required this.media,
    required this.initialIndex,
  });

  final String serverKey;
  final List<Attachment> media;
  final int initialIndex;

  @override
  ConsumerState<MediaViewer> createState() => _MediaViewerState();
}

class _MediaViewerState extends ConsumerState<MediaViewer> {
  late int _currentIndex;
  final TransformationController _transformController =
      TransformationController();
  final FocusNode _focusNode = FocusNode();
  PlaybackNotifier? _playbackNotifier;

  /// The picture shown now: a click beside it closes the viewer.
  final _pictureKey = GlobalKey();

  void _close() => Navigator.of(context).pop();

  /// Whether [global] falls on the picture, as zoomed and moved.
  bool _onPicture(Offset global) {
    final box = _pictureKey.currentContext?.findRenderObject();
    if (box is! RenderBox || !box.hasSize) return false;
    final rect = MatrixUtils.transformRect(
      box.getTransformTo(null),
      Offset.zero & box.size,
    );
    return rect.contains(global);
  }

  @override
  void initState() {
    super.initState();
    _currentIndex = widget.initialIndex.clamp(0, widget.media.length - 1);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _playbackNotifier = ref.read(playbackProvider.notifier);
  }

  @override
  void dispose() {
    _playbackNotifier?.mediaCloseOnDispose();
    _focusNode.dispose();
    _transformController.dispose();
    super.dispose();
  }

  void _previous() {
    if (_currentIndex > 0) {
      ref.read(playbackProvider.notifier).mediaClose();
      setState(() {
        _currentIndex--;
        _transformController.value = Matrix4.identity();
      });
    }
  }

  void _next() {
    if (_currentIndex < widget.media.length - 1) {
      ref.read(playbackProvider.notifier).mediaClose();
      setState(() {
        _currentIndex++;
        _transformController.value = Matrix4.identity();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final current = widget.media[_currentIndex];

    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): _close,
        const SingleActivator(LogicalKeyboardKey.arrowLeft): _previous,
        const SingleActivator(LogicalKeyboardKey.arrowRight): _next,
      },
      child: Focus(
        focusNode: _focusNode,
        autofocus: true,
        child: Scaffold(
          backgroundColor: colors.scrim,
          // Controls take their own clicks; any other click beside the
          // picture closes the viewer.
          body: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapUp: (details) {
              if (!_onPicture(details.globalPosition)) _close();
            },
            child: Stack(
              fit: StackFit.expand,
              children: [
                // Media Content (centered)
                // InteractiveViewer zooms on the mouse wheel itself.
                Center(
                  child: InteractiveViewer(
                    transformationController: _transformController,
                    minScale: 1.0,
                    maxScale: 8.0,
                    child: _buildMediaContent(context, current),
                  ),
                ),

                // Previous button (left edge)
                if (_currentIndex > 0)
                  Positioned(
                    left: OcSpace.s16,
                    top: 0,
                    bottom: 0,
                    child: Center(
                      child: IconButton(
                        icon: const Icon(OcIcons.chevronLeft, size: 32),
                        color: colors.text,
                        tooltip: 'Previous',
                        onPressed: _previous,
                      ),
                    ),
                  ),

                // Next button (right edge)
                if (_currentIndex < widget.media.length - 1)
                  Positioned(
                    right: OcSpace.s16,
                    top: 0,
                    bottom: 0,
                    child: Center(
                      child: IconButton(
                        icon: const Icon(OcIcons.chevronRight, size: 32),
                        color: colors.text,
                        tooltip: 'Next',
                        onPressed: _next,
                      ),
                    ),
                  ),

                // Top Bar; a click on it stays on it.
                Positioned(
                  top: 0,
                  left: 0,
                  right: 0,
                  child: GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onTap: () {},
                    child: Container(
                      height: 56,
                      padding: const EdgeInsets.symmetric(
                        horizontal: OcSpace.s16,
                      ),
                      color: colors.scrim,
                      child: Row(
                        children: [
                          Expanded(
                            child: Text(
                              middleEllipsis(current.filename, maxLength: 36),
                              style: OcText.bodyStrong.copyWith(
                                color: colors.text,
                              ),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          const SizedBox(width: OcSpace.s16),
                          Text(
                            '${_currentIndex + 1} / ${widget.media.length}',
                            style: OcText.body.copyWith(
                              color: colors.textSecondary,
                            ),
                          ),
                          const SizedBox(width: OcSpace.s16),
                          OcButton.ghost(
                            label: 'Download',
                            dense: true,
                            onPressed: () => saveAttachment(
                              context,
                              ref,
                              widget.serverKey,
                              current,
                            ),
                          ),
                          const SizedBox(width: OcSpace.s8),
                          OverlayWindowButtons(onClose: _close),
                        ],
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildMediaContent(BuildContext context, Attachment attachment) {
    final colors = context.oc;

    if (attachment.kind == AttachmentKind.video) {
      return VideoPlayerView(
        serverKey: widget.serverKey,
        attachment: attachment,
        pictureKey: _pictureKey,
      );
    }

    // Image: load original bytes
    final key = (server: widget.serverKey, id: attachment.id, thumbnail: false);
    final bytesAsync = ref.watch(attachmentBytesProvider(key));

    return bytesAsync.when(
      loading: () => const SizedBox(
        width: 64,
        height: 64,
        child: CircularProgressIndicator(strokeWidth: 2),
      ),
      error: (error, _) => Container(
        key: _pictureKey,
        padding: const EdgeInsets.all(OcSpace.s16),
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: BorderRadius.circular(OcRadius.bubble),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              OcIcons.error,
              size: OcSize.iconButton,
              color: colors.textSecondary,
            ),
            const SizedBox(height: OcSpace.s8),
            Text(
              "Couldn't load · Retry",
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
            const SizedBox(height: OcSpace.s8),
            OcButton.ghost(
              label: 'Retry',
              dense: true,
              onPressed: () => ref.invalidate(attachmentBytesProvider(key)),
            ),
          ],
        ),
      ),
      data: (bytes) {
        if (bytes.isEmpty) {
          return const SizedBox.shrink();
        }
        return Image.memory(
          key: _pictureKey,
          bytes,
          fit: BoxFit.contain,
          errorBuilder: (context, error, stackTrace) => Container(
            padding: const EdgeInsets.all(OcSpace.s16),
            child: Icon(OcIcons.image, size: 48, color: colors.textSecondary),
          ),
        );
      },
    );
  }
}
