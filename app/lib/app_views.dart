import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/chat/attachments/media_viewer.dart';
import 'package:opencord/features/media_window/media_viewer_session.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

/// The app's views (D86): the main window's, and while it is open the
/// media viewer's own window, a second view on the same engine. Sits under
/// the app's provider scope, so both views share it.
class AppViews extends StatefulWidget {
  const AppViews({super.key, required this.app, this.findView});

  /// What the main window shows.
  final Widget app;

  /// The Flutter view with this id, once the runner has added it; by
  /// default, among the platform's views.
  final FlutterView? Function(int viewId)? findView;

  @override
  State<AppViews> createState() => _AppViewsState();
}

class _AppViewsState extends State<AppViews> with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  /// Views come and go as metrics changes: the viewer's appears here.
  @override
  void didChangeMetrics() => setState(() {});

  FlutterView? _find(int viewId) {
    final find = widget.findView;
    if (find != null) return find(viewId);
    return WidgetsBinding.instance.platformDispatcher.view(id: viewId);
  }

  @override
  Widget build(BuildContext context) {
    final implicit = WidgetsBinding.instance.platformDispatcher.implicitView!;
    return Consumer(
      builder: (context, ref, _) {
        final viewId = ref.watch(
          mediaViewerProvider.select((session) => session?.viewId),
        );
        final viewer = viewId == null ? null : _find(viewId);
        return ViewCollection(
          views: [
            View(view: implicit, child: widget.app),
            if (viewer != null)
              View(view: viewer, child: const MediaViewerWindow()),
          ],
        );
      },
    );
  }
}

/// The media viewer's window: the viewer, dark in either theme, starting
/// again at its item with each new request.
class MediaViewerWindow extends ConsumerWidget {
  const MediaViewerWindow({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final session = ref.watch(mediaViewerProvider);
    if (session == null) return const SizedBox.shrink();
    final request = session.request;
    return MaterialApp(
      title: 'Opencord media viewer',
      debugShowCheckedModeBanner: false,
      theme: buildTheme(OcColors.dark),
      // The window itself is see-through; the viewer paints the dark.
      color: Colors.transparent,
      home: MediaViewer(
        key: ValueKey(session.generation),
        serverKey: request.serverKey,
        media: request.media,
        initialIndex: request.index,
        senderName: request.senderName,
        sentAt: request.sentAt,
        inWindow: true,
        mode: session.status.mode,
        onClose: ref.read(mediaViewerProvider.notifier).close,
      ),
    );
  }
}
