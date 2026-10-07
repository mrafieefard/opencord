import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/onboarding/onboarding_view.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/gallery/widget_gallery.dart';

final routerProvider = Provider<GoRouter>((ref) {
  final router = GoRouter(
    routes: [
      GoRoute(path: '/', builder: (context, state) => const _Home()),
      GoRoute(
        path: '/gallery',
        builder: (context, state) => const WidgetGallery(),
      ),
    ],
  );
  ref.onDispose(router.dispose);
  return router;
});

/// Onboarding until there is an identity, then the app (Phase 1 §9.1).
class _Home extends ConsumerWidget {
  const _Home();

  @override
  Widget build(BuildContext context, WidgetRef ref) =>
      ref.watch(localIdentityProvider) == null
      ? const OnboardingView()
      : const DesktopShell();
}
