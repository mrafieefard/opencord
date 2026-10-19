import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

Future<void> showMediaViewer(
  BuildContext context, {
  required String serverKey,
  required List<Attachment> media,
  required int index,
}) {
  final colors = context.oc;
  return Navigator.of(context, rootNavigator: true).push<void>(
    PageRouteBuilder<void>(
      opaque: false,
      barrierDismissible: true,
      barrierColor: colors.scrim,
      pageBuilder: (context, animation, secondaryAnimation) =>
          MediaViewer(serverKey: serverKey, media: media, initialIndex: index),
      transitionsBuilder: (context, animation, secondaryAnimation, child) {
        return FadeTransition(opacity: animation, child: child);
      },
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

  @override
  void initState() {
    super.initState();
    _currentIndex = widget.initialIndex.clamp(0, widget.media.length - 1);
  }

  @override
  void dispose() {
    _focusNode.dispose();
    _transformController.dispose();
    super.dispose();
  }

  void _previous() {
    if (_currentIndex > 0) {
      setState(() {
        _currentIndex--;
        _transformController.value = Matrix4.identity();
      });
    }
  }

  void _next() {
    if (_currentIndex < widget.media.length - 1) {
      setState(() {
        _currentIndex++;
        _transformController.value = Matrix4.identity();
      });
    }
  }

  void _handlePointerSignal(PointerSignalEvent event) {
    if (event is PointerScrollEvent) {
      final double zoomFactor = event.scrollDelta.dy < 0 ? 1.15 : 0.85;
      final currentScale = _transformController.value.getMaxScaleOnAxis();
      final targetScale = (currentScale * zoomFactor).clamp(1.0, 8.0);
      final effectiveFactor = targetScale / currentScale;

      final focalPoint = event.localPosition;
      final Matrix4 matrix = _transformController.value.clone();
      matrix.translateByDouble(focalPoint.dx, focalPoint.dy, 0.0, 1.0);
      matrix.scaleByDouble(effectiveFactor, effectiveFactor, 1.0, 1.0);
      matrix.translateByDouble(-focalPoint.dx, -focalPoint.dy, 0.0, 1.0);
      _transformController.value = matrix;
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final current = widget.media[_currentIndex];

    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () {
          Navigator.of(context).pop();
        },
        const SingleActivator(LogicalKeyboardKey.arrowLeft): _previous,
        const SingleActivator(LogicalKeyboardKey.arrowRight): _next,
      },
      child: Focus(
        focusNode: _focusNode,
        autofocus: true,
        child: Scaffold(
          backgroundColor: Colors.transparent,
          body: Stack(
            fit: StackFit.expand,
            children: [
              // Media Content (centered)
              Center(
                child: Listener(
                  onPointerSignal: _handlePointerSignal,
                  child: InteractiveViewer(
                    transformationController: _transformController,
                    minScale: 1.0,
                    maxScale: 8.0,
                    child: _buildMediaContent(context, current),
                  ),
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
                      color: OcColors.dark.text,
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
                      color: OcColors.dark.text,
                      tooltip: 'Next',
                      onPressed: _next,
                    ),
                  ),
                ),

              // Top Bar
              Positioned(
                top: 0,
                left: 0,
                right: 0,
                child: Container(
                  height: 56,
                  padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
                  color: colors.scrim,
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          middleEllipsis(current.filename, maxLength: 36),
                          style: OcText.bodyStrong.copyWith(
                            color: OcColors.dark.text,
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
                      IconButton(
                        icon: const Icon(OcIcons.close),
                        color: OcColors.dark.text,
                        tooltip: 'Close',
                        onPressed: () => Navigator.of(context).pop(),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildMediaContent(BuildContext context, Attachment attachment) {
    final colors = context.oc;

    if (attachment.kind == AttachmentKind.video) {
      return Container(
        width: 480,
        height: 320,
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: BorderRadius.circular(OcRadius.bubble),
          border: Border.all(color: colors.border),
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(OcIcons.movie, size: 48, color: colors.textSecondary),
            const SizedBox(height: OcSpace.s12),
            Text(
              'Video playback coming soon',
              style: OcText.body.copyWith(color: colors.textSecondary),
            ),
          ],
        ),
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
