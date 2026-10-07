import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/gallery/widget_gallery.dart';

final routerProvider = Provider<GoRouter>((ref) {
  final router = GoRouter(
    routes: [
      GoRoute(path: '/', builder: (context, state) => const DesktopShell()),
      GoRoute(
        path: '/gallery',
        builder: (context, state) => const WidgetGallery(),
      ),
    ],
  );
  ref.onDispose(router.dispose);
  return router;
});
