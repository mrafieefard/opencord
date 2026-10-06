import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// Soft shadows for the few places that have one (§1: almost no shadows).
abstract final class OcShadows {
  static const Color _shadow = Color(0xFF000000);

  /// Around the custom-drawn Linux window frame.
  static const List<BoxShadow> window = [
    BoxShadow(color: Color(0x40000000), blurRadius: 18, offset: Offset(0, 4)),
  ];

  /// Under menus and popovers.
  static List<BoxShadow> menu(OcColors colors) => [
    BoxShadow(
      color: _shadow.withValues(alpha: colors == OcColors.dark ? 0.5 : 0.12),
      blurRadius: 16,
      offset: const Offset(0, 4),
    ),
  ];
}

/// Builds the Material theme from the tokens, so that the few Material
/// widgets the app uses are monochrome too.
ThemeData buildTheme(OcColors colors) {
  final brightness = colors.text.computeLuminance() > 0.5
      ? Brightness.dark
      : Brightness.light;
  final scheme = ColorScheme(
    brightness: brightness,
    primary: colors.accent,
    onPrimary: colors.onAccent,
    primaryContainer: colors.selected,
    onPrimaryContainer: colors.text,
    secondary: colors.accent,
    onSecondary: colors.onAccent,
    secondaryContainer: colors.selected,
    onSecondaryContainer: colors.text,
    tertiary: colors.accent,
    onTertiary: colors.onAccent,
    tertiaryContainer: colors.selected,
    onTertiaryContainer: colors.text,
    error: colors.text,
    onError: colors.onAccent,
    errorContainer: colors.selected,
    onErrorContainer: colors.text,
    surface: colors.surface,
    onSurface: colors.text,
    onSurfaceVariant: colors.textSecondary,
    surfaceDim: colors.chat,
    surfaceBright: colors.elevated,
    surfaceContainerLowest: colors.chat,
    surfaceContainerLow: colors.sidebar,
    surfaceContainer: colors.surface,
    surfaceContainerHigh: colors.elevated,
    surfaceContainerHighest: colors.selected,
    outline: colors.border,
    outlineVariant: colors.border,
    shadow: OcShadows._shadow,
    scrim: colors.scrim,
    inverseSurface: colors.accent,
    onInverseSurface: colors.onAccent,
    inversePrimary: colors.onAccent,
    surfaceTint: Colors.transparent,
  );
  final base = OcText.body.copyWith(color: colors.text);
  final textTheme = TextTheme(
    displayLarge: OcText.title.copyWith(color: colors.text, fontSize: 32),
    displayMedium: OcText.title.copyWith(color: colors.text, fontSize: 26),
    displaySmall: OcText.title.copyWith(color: colors.text, fontSize: 22),
    headlineLarge: OcText.title.copyWith(color: colors.text, fontSize: 22),
    headlineMedium: OcText.title.copyWith(color: colors.text, fontSize: 19),
    headlineSmall: OcText.title.copyWith(color: colors.text),
    titleLarge: OcText.title.copyWith(color: colors.text),
    titleMedium: OcText.header.copyWith(color: colors.text),
    titleSmall: OcText.bodyStrong.copyWith(color: colors.text),
    bodyLarge: base,
    bodyMedium: base,
    bodySmall: OcText.small.copyWith(color: colors.textSecondary),
    labelLarge: OcText.bodyStrong.copyWith(color: colors.text),
    labelMedium: OcText.small.copyWith(color: colors.textSecondary),
    labelSmall: OcText.meta.copyWith(color: colors.textMuted),
  );

  return ThemeData(
    useMaterial3: true,
    brightness: brightness,
    colorScheme: scheme,
    extensions: [colors],
    fontFamily: OcText.fontFamily,
    textTheme: textTheme,
    primaryTextTheme: textTheme,
    scaffoldBackgroundColor: colors.chat,
    canvasColor: colors.sidebar,
    cardColor: colors.surface,
    dividerColor: colors.border,
    dividerTheme: DividerThemeData(
      color: colors.border,
      thickness: 1,
      space: 1,
    ),
    hoverColor: colors.hover,
    focusColor: colors.selected,
    highlightColor: Colors.transparent,
    splashColor: Colors.transparent,
    splashFactory: NoSplash.splashFactory,
    disabledColor: colors.textMuted,
    iconTheme: IconThemeData(
      color: colors.text,
      size: OcSize.iconButton,
      fill: 0,
      weight: 400,
      grade: 0,
      opticalSize: 20,
    ),
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: colors.text,
      selectionColor: colors.text.withValues(alpha: 0.22),
      selectionHandleColor: colors.text,
    ),
    tooltipTheme: TooltipThemeData(
      waitDuration: const Duration(milliseconds: 450),
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s8,
        vertical: OcSpace.s6,
      ),
      decoration: BoxDecoration(
        color: colors.elevated,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: colors.border),
      ),
      textStyle: OcText.small.copyWith(color: colors.text),
    ),
    scrollbarTheme: ScrollbarThemeData(
      thickness: WidgetStateProperty.resolveWith(
        (states) => states.contains(WidgetState.hovered) ? 8 : 4,
      ),
      radius: const Radius.circular(4),
      crossAxisMargin: 2,
      mainAxisMargin: 2,
      thumbColor: WidgetStateProperty.resolveWith(
        (states) => colors.textMuted.withValues(
          alpha: states.contains(WidgetState.dragged) ? 0.8 : 0.5,
        ),
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: colors.hover,
      isDense: true,
      contentPadding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s12,
        vertical: OcSpace.s10,
      ),
      hintStyle: OcText.body.copyWith(color: colors.textMuted),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(OcRadius.input),
        borderSide: BorderSide.none,
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(OcRadius.input),
        borderSide: BorderSide(color: colors.border),
      ),
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: colors.elevated,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(OcRadius.dialog),
        side: BorderSide(color: colors.border),
      ),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(
      color: colors.text,
      linearTrackColor: colors.selected,
      circularTrackColor: Colors.transparent,
    ),
    pageTransitionsTheme: const PageTransitionsTheme(
      builders: {
        TargetPlatform.linux: _NoTransition(),
        TargetPlatform.windows: _NoTransition(),
        TargetPlatform.macOS: _NoTransition(),
        TargetPlatform.android: _NoTransition(),
        TargetPlatform.iOS: _NoTransition(),
        TargetPlatform.fuchsia: _NoTransition(),
      },
    ),
    visualDensity: VisualDensity.standard,
    materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
  );
}

/// Desktop pages swap without a slide or fade.
class _NoTransition extends PageTransitionsBuilder {
  const _NoTransition();

  @override
  Widget buildTransitions<T>(
    PageRoute<T> route,
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) => child;
}
