import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

import '../../support/colors.dart';

void main() {
  for (final (name, colors, brightness) in [
    ('dark', OcColors.dark, Brightness.dark),
    ('light', OcColors.light, Brightness.light),
  ]) {
    group('$name theme', () {
      final theme = buildTheme(colors);

      test('carries the tokens and the matching brightness', () {
        expect(theme.extension<OcColors>(), colors);
        expect(theme.brightness, brightness);
        expect(theme.scaffoldBackgroundColor, colors.chat);
        expect(theme.dividerTheme.color, colors.border);
      });

      test('the Material color scheme has no hue', () {
        final scheme = theme.colorScheme;
        final schemeColors = <String, Color>{
          'primary': scheme.primary,
          'onPrimary': scheme.onPrimary,
          'primaryContainer': scheme.primaryContainer,
          'secondary': scheme.secondary,
          'secondaryContainer': scheme.secondaryContainer,
          'tertiary': scheme.tertiary,
          'error': scheme.error,
          'onError': scheme.onError,
          'errorContainer': scheme.errorContainer,
          'surface': scheme.surface,
          'onSurface': scheme.onSurface,
          'onSurfaceVariant': scheme.onSurfaceVariant,
          'surfaceContainerHighest': scheme.surfaceContainerHighest,
          'outline': scheme.outline,
          'outlineVariant': scheme.outlineVariant,
          'inverseSurface': scheme.inverseSurface,
          'shadow': scheme.shadow,
        };
        for (final MapEntry(:key, :value) in schemeColors.entries) {
          expect(isNeutral(value), isTrue, reason: key);
        }
        expect(
          scheme.surfaceTint.a,
          0,
          reason: 'no Material 3 tint on surfaces',
        );
      });

      test('uses the bundled fonts', () {
        expect(theme.textTheme.bodyMedium?.fontFamily, 'Inter');
        expect(theme.iconTheme.size, 20);
        expect(theme.iconTheme.color, colors.text);
      });
    });
  }

  testWidgets('context.oc reads the tokens of the current theme', (
    tester,
  ) async {
    late OcColors seen;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(OcColors.light),
        home: Builder(
          builder: (context) {
            seen = context.oc;
            return const SizedBox();
          },
        ),
      ),
    );

    expect(seen, OcColors.light);
  });
}
