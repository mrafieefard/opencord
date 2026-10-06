import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/app_router.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

export 'package:opencord/app_router.dart' show routerProvider;

final _darkTheme = buildTheme(OcColors.dark);
final _lightTheme = buildTheme(OcColors.light);

/// Root widget of the Opencord client.
class OpencordApp extends ConsumerWidget {
  const OpencordApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = ref.watch(appSettingsProvider.select((s) => s.theme));
    return MaterialApp.router(
      title: 'Opencord',
      debugShowCheckedModeBanner: false,
      routerConfig: ref.watch(routerProvider),
      theme: _lightTheme,
      darkTheme: _darkTheme,
      themeMode: switch (theme) {
        ThemePreference.system => ThemeMode.system,
        ThemePreference.dark => ThemeMode.dark,
        ThemePreference.light => ThemeMode.light,
      },
      themeAnimationDuration: Duration.zero,
      builder: (context, child) => _AppMedia(child: child!),
    );
  }
}

/// Applies the font size and reduce-motion settings to everything below.
class _AppMedia extends ConsumerWidget {
  const _AppMedia({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final textScale = ref.watch(appSettingsProvider.select((s) => s.textScale));
    final reduceMotion = ref.watch(
      appSettingsProvider.select((s) => s.reduceMotion),
    );
    final media = MediaQuery.of(context);
    final platformScale = media.textScaler.scale(1);
    return MediaQuery(
      data: media.copyWith(
        textScaler: TextScaler.linear(platformScale * textScale),
        disableAnimations: media.disableAnimations || reduceMotion,
      ),
      child: child,
    );
  }
}
