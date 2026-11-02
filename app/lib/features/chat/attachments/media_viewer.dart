import 'dart:async';
import 'dart:io';

import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/providers/playback_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/features/chat/attachments/video_player_view.dart';
import 'package:opencord/features/media_window/media_viewer_session.dart';
import 'package:opencord/features/media_window/media_window_buttons.dart';
import 'package:opencord/features/media_window/media_window_service.dart';
import 'package:opencord/features/window/window_buttons.dart';
import 'package:opencord/features/window/window_frame.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_media_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:url_launcher/url_launcher.dart';

/// Shows [media] from [index] in the viewer's own window (D86), or, where
/// there is no such window, as a see-through layer over the whole app
/// window, title bar included, that appears and goes at once.
Future<void> showMediaViewer(
  BuildContext context, {
  required String serverKey,
  required List<Attachment> media,
  required int index,
  String? senderName,
  DateTime? sentAt,
}) async {
  final container = ProviderScope.containerOf(context, listen: false);
  // No window to open (tests, a runner without one): nothing to ask.
  if (container.read(mediaWindowServiceProvider) is! NullMediaWindow) {
    final opened = await container
        .read(mediaViewerProvider.notifier)
        .open(
          MediaViewerRequest(
            serverKey: serverKey,
            media: media,
            index: index,
            senderName: senderName,
            sentAt: sentAt,
          ),
        );
    if (opened) return;
  }
  if (!context.mounted) return;
  return Navigator.of(context, rootNavigator: true).push<void>(
    PageRouteBuilder<void>(
      opaque: false,
      transitionDuration: Duration.zero,
      reverseTransitionDuration: Duration.zero,
      // Dark in either theme: everything here sits on the dark background.
      pageBuilder: (context, animation, secondaryAnimation) => Theme(
        data: buildTheme(OcColors.dark),
        child: MediaViewer(
          serverKey: serverKey,
          media: media,
          initialIndex: index,
          senderName: senderName,
          sentAt: sentAt,
          onClose: () => Navigator.of(context).pop(),
        ),
      ),
    ),
  );
}

/// "Photo 3 of 7" or "Video 2 of 3"; the file name when there is one item.
String mediaHeader(Attachment current, int index, int count) {
  if (count == 1) return current.filename;
  final kind = current.kind == AttachmentKind.video ? 'Video' : 'Photo';
  return '$kind ${index + 1} of $count';
}

/// The media viewer, laid out as Telegram's (D86): the media in the middle,
/// previous and next along the sides, what it is and who sent it at the
/// bottom left, Download at the bottom right, and controls that fade when
/// the mouse rests. [inWindow] is the viewer's own window, with its title
/// buttons and Telegram's rules for its two [mode]s.
class MediaViewer extends ConsumerStatefulWidget {
  const MediaViewer({
    super.key,
    required this.serverKey,
    required this.media,
    required this.initialIndex,
    required this.onClose,
    this.senderName,
    this.sentAt,
    this.inWindow = false,
    this.mode = MediaWindowMode.fullscreen,
  });

  final String serverKey;
  final List<Attachment> media;
  final int initialIndex;
  final String? senderName;
  final DateTime? sentAt;
  final VoidCallback onClose;
  final bool inWindow;
  final MediaWindowMode mode;

  @override
  ConsumerState<MediaViewer> createState() => _MediaViewerState();
}

class _MediaViewerState extends ConsumerState<MediaViewer> {
  static const _restBeforeFade = Duration(milliseconds: 1100);
  static const _fadeOut = Duration(milliseconds: 600);
  static const _fadeIn = Duration(milliseconds: 200);
  static const _noteShown = Duration(seconds: 2);

  /// Telegram's measures: the side strips, the text at the bottom left,
  /// the icons at the bottom right and the title strip.
  static const double _navWidth = 90;
  static const double _navInset = 47;
  static const double _textLeft = 14;
  static const double _titleHeight = 32;
  static const double _bottomRow = 54;

  late int _currentIndex;
  final TransformationController _transformController =
      TransformationController();
  final FocusNode _focusNode = FocusNode();
  PlaybackNotifier? _playbackNotifier;

  /// The picture shown now: a click beside it closes the viewer.
  final _pictureKey = GlobalKey();

  /// What the zoom scales: the whole viewer, the picture in its middle.
  final _zoomedKey = GlobalKey();

  /// Quarter turns of the picture, counter-clockwise ones as three.
  int _quarterTurns = 0;

  bool _controlsShown = true;
  Timer? _fadeTimer;
  bool _pointerHeld = false;
  int _overControls = 0;

  String? _note;
  String? _noteFolder;
  Timer? _noteTimer;

  bool get _normalWindow =>
      widget.inWindow && widget.mode == MediaWindowMode.normal;

  void _close() => widget.onClose();

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

  /// [picture] zoomable across the whole viewer, as Telegram's: the wheel,
  /// a pinch or a drag on the picture zoom and move it, and anywhere else
  /// stays the viewer's (a click beside it, a drag that moves the window).
  /// Unzoomed, the picture keeps clear of the bottom row, so a video's
  /// controls are never over it, and the same at the top keeps it centred.
  Widget _zoom(Widget picture) => _PictureHits(
    hits: _onPicture,
    child: InteractiveViewer(
      transformationController: _transformController,
      minScale: 1.0,
      maxScale: 8.0,
      // The picture's own edges bound it, in _keepOnScreen.
      boundaryMargin: const EdgeInsets.all(double.infinity),
      child: Padding(
        key: _zoomedKey,
        padding: const EdgeInsets.symmetric(vertical: _bottomRow),
        child: Center(
          child: RotatedBox(quarterTurns: _quarterTurns, child: picture),
        ),
      ),
    ),
  );

  /// After each zoom or move: along each side, a picture smaller than the
  /// viewer is centred, and a bigger one leaves no gap at its edges.
  void _keepOnScreen() {
    final picture = _pictureKey.currentContext?.findRenderObject();
    final zoomed = _zoomedKey.currentContext?.findRenderObject();
    if (picture is! RenderBox || zoomed is! RenderBox) return;
    if (!picture.hasSize || !zoomed.hasSize) return;
    final rect = MatrixUtils.transformRect(
      picture.getTransformTo(zoomed),
      Offset.zero & picture.size,
    );
    final matrix = _transformController.value;
    final scale = matrix.getMaxScaleOnAxis();
    final moved = matrix.getTranslation();

    double place(double start, double end, double viewer, double now) {
      final length = (end - start) * scale;
      if (length <= viewer) return (viewer - length) / 2 - start * scale;
      return now.clamp(viewer - end * scale, -start * scale);
    }

    final x = place(rect.left, rect.right, zoomed.size.width, moved.x);
    final y = place(rect.top, rect.bottom, zoomed.size.height, moved.y);
    if ((x - moved.x).abs() < 0.01 && (y - moved.y).abs() < 0.01) return;
    _transformController.value = matrix.clone()
      ..setTranslationRaw(x, y, moved.z);
  }

  @override
  void initState() {
    super.initState();
    _currentIndex = widget.initialIndex.clamp(0, widget.media.length - 1);
    _transformController.addListener(_keepOnScreen);
    _restartFade();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _playbackNotifier = ref.read(playbackProvider.notifier);
  }

  @override
  void dispose() {
    _playbackNotifier?.mediaCloseOnDispose();
    _fadeTimer?.cancel();
    _noteTimer?.cancel();
    _focusNode.dispose();
    _transformController.dispose();
    super.dispose();
  }

  /// The mouse moved: controls come back and fade again once it rests.
  void _activity() {
    if (!_controlsShown) setState(() => _controlsShown = true);
    _restartFade();
  }

  void _restartFade() {
    _fadeTimer?.cancel();
    _fadeTimer = Timer(_restBeforeFade, () {
      if (!mounted || _pointerHeld || _overControls > 0) return;
      setState(() => _controlsShown = false);
    });
  }

  /// A control the pointer is over keeps the controls up.
  Widget _control(Widget child) => MouseRegion(
    onEnter: (_) => _overControls++,
    onExit: (_) {
      if (_overControls > 0) _overControls--;
      _restartFade();
    },
    child: child,
  );

  void _toggleMode() {
    if (!widget.inWindow) return;
    ref.read(mediaViewerProvider.notifier).toggleMode();
  }

  void _previous() {
    if (_currentIndex > 0) {
      ref.read(playbackProvider.notifier).mediaClose();
      setState(() {
        _currentIndex--;
        _quarterTurns = 0;
        _transformController.value = Matrix4.identity();
      });
    }
  }

  void _next() {
    if (_currentIndex < widget.media.length - 1) {
      ref.read(playbackProvider.notifier).mediaClose();
      setState(() {
        _currentIndex++;
        _quarterTurns = 0;
        _transformController.value = Matrix4.identity();
      });
    }
  }

  /// A quarter turn left, as Telegram's: the zoom starts again.
  void _rotate() => setState(() {
    _quarterTurns = (_quarterTurns + 3) % 4;
    _transformController.value = Matrix4.identity();
  });

  Future<void> _download(Attachment attachment) async {
    final repository = ref.read(repositoryProvider);
    try {
      final path = await saveToDownloads(
        repository,
        widget.serverKey,
        attachment,
      );
      _showNote(
        path == null ? "There's no Downloads folder" : 'Saved to Downloads',
        folder: path == null ? null : File(path).parent.path,
      );
    } on RepoException catch (error) {
      if (error.kind == RepoErrorKind.cancelled) return;
      _showNote(error.message);
    }
  }

  void _showNote(String text, {String? folder}) {
    if (!mounted) return;
    _noteTimer?.cancel();
    setState(() {
      _note = text;
      _noteFolder = folder;
    });
    _noteTimer = Timer(_noteShown, () {
      if (mounted) setState(() => _note = null);
    });
  }

  @override
  Widget build(BuildContext context) {
    final current = widget.media[_currentIndex];
    final closesOnClick = !_normalWindow;

    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): _close,
        const SingleActivator(LogicalKeyboardKey.arrowLeft): _previous,
        const SingleActivator(LogicalKeyboardKey.arrowRight): _next,
        const SingleActivator(LogicalKeyboardKey.enter, alt: true): _toggleMode,
        const SingleActivator(LogicalKeyboardKey.enter, control: true):
            _toggleMode,
        const SingleActivator(LogicalKeyboardKey.numpadEnter, alt: true):
            _toggleMode,
      },
      child: Focus(
        focusNode: _focusNode,
        autofocus: true,
        child: Listener(
          onPointerDown: (_) {
            _pointerHeld = true;
            _activity();
          },
          onPointerUp: (_) {
            _pointerHeld = false;
            _restartFade();
          },
          // A move or resize the system took over may end this way.
          onPointerCancel: (_) {
            _pointerHeld = false;
            _restartFade();
          },
          child: MouseRegion(
            onHover: (_) => _activity(),
            child: Material(
              color: OcMediaColors.background,
              // Controls take their own clicks; any other click beside
              // the picture closes the viewer, except in a normal window,
              // where dragging the background moves it.
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTapUp: (details) {
                  if (closesOnClick && !_onPicture(details.globalPosition)) {
                    _close();
                  }
                },
                onPanStart: _normalWindow
                    ? (_) => ref.read(mediaWindowServiceProvider).startDrag()
                    : null,
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    Center(child: _buildMediaContent(context, current)),
                    if (widget.inWindow)
                      Positioned(
                        top: 0,
                        left: 0,
                        right: 0,
                        height: _titleHeight,
                        child: GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          // macOS's own title bar zooms on a double click,
                          // which the runner turns into the switch.
                          onDoubleTap:
                              Theme.of(context).platform == TargetPlatform.macOS
                              ? null
                              : _toggleMode,
                          onPanStart: _normalWindow
                              ? (_) => ref
                                    .read(mediaWindowServiceProvider)
                                    .startDrag()
                              : null,
                        ),
                      ),
                    Positioned.fill(
                      child: AnimatedOpacity(
                        opacity: _controlsShown ? 1 : 0,
                        duration: _controlsShown ? _fadeIn : _fadeOut,
                        child: _buildControls(context, current),
                      ),
                    ),
                    if (_note case final note?) _buildNote(context, note),
                    // The window has no frame on Linux: it resizes from
                    // its edges.
                    if (_normalWindow &&
                        Theme.of(context).platform == TargetPlatform.linux)
                      ...windowResizeHandles(
                        onResize: ref
                            .read(mediaWindowServiceProvider)
                            .startResize,
                        outside: 0,
                        band: windowResizeBand,
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildControls(BuildContext context, Attachment current) {
    final colors = context.oc;
    final now = clock.now();
    final sentAt = widget.sentAt;
    final from = [
      ?widget.senderName,
      if (sentAt != null) '${shortDate(sentAt, now)} at ${clockTime(sentAt)}',
    ].join(' · ');
    return Stack(
      children: [
        if (_currentIndex > 0)
          Positioned(
            left: 0,
            top: _navInset,
            bottom: _navInset,
            width: _navWidth,
            child: _control(
              _NavStrip(
                icon: OcIcons.chevronLeft,
                tooltip: 'Previous',
                onPressed: _previous,
              ),
            ),
          ),
        if (_currentIndex < widget.media.length - 1)
          Positioned(
            right: 0,
            top: _navInset,
            bottom: _navInset,
            width: _navWidth,
            child: _control(
              _NavStrip(
                icon: OcIcons.chevronRight,
                tooltip: 'Next',
                onPressed: _next,
              ),
            ),
          ),
        Positioned(
          left: _textLeft,
          bottom: 14,
          right: 2 * _navWidth,
          child: IgnorePointer(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  mediaHeader(current, _currentIndex, widget.media.length),
                  style: OcText.bodyStrong.copyWith(color: colors.text),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                if (from.isNotEmpty) ...[
                  const SizedBox(height: OcSpace.s4),
                  Text(
                    from,
                    style: OcText.small.copyWith(color: colors.textSecondary),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
              ],
            ),
          ),
        ),
        Positioned(
          right: 0,
          bottom: 0,
          child: _control(
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                _bottomButton(
                  OcIcons.download,
                  'Download',
                  () => _download(current),
                ),
                _bottomButton(OcIcons.rotateLeft, 'Rotate', _rotate),
              ],
            ),
          ),
        ),
        if (widget.inWindow) ...[
          Positioned(
            top: 0,
            left: 0,
            child: _control(
              MediaWindowButtons(side: ButtonSide.left, onClose: _close),
            ),
          ),
          Positioned(
            top: 0,
            right: 0,
            child: _control(
              MediaWindowButtons(side: ButtonSide.right, onClose: _close),
            ),
          ),
        ] else
          Positioned(
            top: OcSpace.s8,
            right: OcSpace.s8,
            child: _control(OverlayWindowButtons(onClose: _close)),
          ),
      ],
    );
  }

  /// One of the media's own buttons at the bottom right, Telegram's size.
  Widget _bottomButton(IconData icon, String tooltip, VoidCallback onPressed) =>
      SizedBox(
        width: 46,
        height: _bottomRow,
        child: IconButton(
          icon: Icon(icon),
          color: context.oc.text,
          tooltip: tooltip,
          onPressed: onPressed,
        ),
      );

  Widget _buildNote(BuildContext context, String note) {
    final colors = context.oc;
    final folder = _noteFolder;
    return Positioned(
      left: 0,
      right: 0,
      bottom: 80,
      child: Center(
        child: GestureDetector(
          onTap: folder == null ? null : () => launchUrl(Uri.directory(folder)),
          child: Container(
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s16,
              vertical: OcSpace.s8,
            ),
            decoration: BoxDecoration(
              color: OcMediaColors.note,
              borderRadius: BorderRadius.circular(OcRadius.bubble),
            ),
            child: Text(note, style: OcText.body.copyWith(color: colors.text)),
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
        framePicture: _zoom,
        controlsInset: _bottomRow,
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
        return _zoom(
          Image.memory(
            key: _pictureKey,
            bytes,
            fit: BoxFit.contain,
            errorBuilder: (context, error, stackTrace) => Container(
              padding: const EdgeInsets.all(OcSpace.s16),
              child: Icon(OcIcons.image, size: 48, color: colors.textSecondary),
            ),
          ),
        );
      },
    );
  }
}

/// Previous or next: a strip along the window's edge with a round hover
/// behind its chevron, as Telegram's.
class _NavStrip extends StatefulWidget {
  const _NavStrip({
    required this.icon,
    required this.tooltip,
    required this.onPressed,
  });

  final IconData icon;
  final String tooltip;
  final VoidCallback onPressed;

  @override
  State<_NavStrip> createState() => _NavStripState();
}

class _NavStripState extends State<_NavStrip> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Semantics(
      button: true,
      label: widget.tooltip,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.onPressed,
          child: Center(
            child: Tooltip(
              message: widget.tooltip,
              child: Container(
                width: 36,
                height: 36,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: _hovered ? OcMediaColors.navHover : null,
                ),
                child: Icon(widget.icon, size: 28, color: colors.text),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Lets hits through only where [hits] says, in global coordinates: the
/// zoom takes the picture's clicks and leaves the rest to the viewer.
class _PictureHits extends SingleChildRenderObjectWidget {
  const _PictureHits({required this.hits, required super.child});

  final bool Function(Offset global) hits;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderPictureHits(hits);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderPictureHits renderObject,
  ) => renderObject.hits = hits;
}

class _RenderPictureHits extends RenderProxyBox {
  _RenderPictureHits(this.hits);

  bool Function(Offset global) hits;

  @override
  bool hitTest(BoxHitTestResult result, {required Offset position}) =>
      hits(localToGlobal(position)) &&
      super.hitTest(result, position: position);
}
