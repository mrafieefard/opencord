import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:opencord/core/app_info.dart';
import 'package:opencord/ui/gallery/widget_gallery.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';

final routerProvider = Provider<GoRouter>((ref) {
  final router = GoRouter(
    routes: [
      GoRoute(path: '/', builder: (context, state) => const _Startup()),
      GoRoute(
        path: '/gallery',
        builder: (context, state) => const WidgetGallery(),
      ),
    ],
  );
  ref.onDispose(router.dispose);
  return router;
});

/// Shown until the main layout exists.
class _Startup extends ConsumerWidget {
  const _Startup();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return ColoredBox(
      color: colors.chat,
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('Opencord', style: OcText.title.copyWith(color: colors.text)),
            const SizedBox(height: 4),
            Text(
              'Core ${ref.watch(coreVersionProvider)}',
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ],
        ),
      ),
    );
  }
}
