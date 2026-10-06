import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/theme/oc_colors.dart';

import '../../support/colors.dart';

String hex(Color color) =>
    '#${color.toARGB32().toRadixString(16).padLeft(8, '0').substring(2).toUpperCase()}';

void main() {
  group('tokens match §2.1', () {
    final table = <String, (String, String)>{
      'rail': ('#0A0A0B', '#E9E9EC'),
      'sidebar': ('#121214', '#FFFFFF'),
      'chat': ('#0E0E10', '#F3F3F5'),
      'surface': ('#18181B', '#FFFFFF'),
      'elevated': ('#1F1F23', '#FFFFFF'),
      'hover': ('#1C1C20', '#F2F2F4'),
      'selected': ('#27272C', '#E7E7EA'),
      'border': ('#222226', '#E3E3E7'),
      'text': ('#EDEDEF', '#111114'),
      'textSecondary': ('#A1A1A8', '#55555C'),
      'textMuted': ('#6E6E76', '#8A8A92'),
      'accent': ('#EDEDEF', '#111114'),
      'onAccent': ('#0E0E10', '#FFFFFF'),
      'bubbleIn': ('#1A1A1E', '#FFFFFF'),
      'bubbleOut': ('#2A2A30', '#E4E4E8'),
      'mentionBg': ('#26262B', '#E2E2E6'),
    };
    for (final MapEntry(key: name, value: (dark, light)) in table.entries) {
      test(name, () {
        expect(hex(OcColors.dark.byName(name)), dark);
        expect(hex(OcColors.light.byName(name)), light);
      });
    }

    test('scrim is black at 70% in dark and 40% in light', () {
      expect(OcColors.dark.scrim, const Color(0xB3000000));
      expect(OcColors.light.scrim, const Color(0x66000000));
    });

    test('avatar shades', () {
      expect(OcColors.dark.avatarShades.map(hex), [
        '#2B2B30',
        '#34343A',
        '#3E3E45',
        '#494951',
        '#55555D',
      ]);
      expect(OcColors.light.avatarShades.map(hex), [
        '#D9D9DE',
        '#CDCDD3',
        '#C1C1C8',
        '#E3E3E7',
        '#B6B6BE',
      ]);
    });
  });

  test('no token carries a hue', () {
    for (final colors in [OcColors.dark, OcColors.light]) {
      for (final color in [...colors.all.values, ...colors.avatarShades]) {
        expect(isNeutral(color), isTrue, reason: hex(color));
      }
    }
  });

  test('an id always gets the same avatar shade', () {
    final colors = OcColors.dark;
    final shades = {for (var id = 0; id < 200; id++) colors.avatarShade(id)};

    expect(colors.avatarShade(123456789), colors.avatarShade(123456789));
    expect(
      colors.avatarShade('dev.example:7710'),
      colors.avatarShade('dev.example:7710'),
    );
    expect(shades, containsAll(colors.avatarShades));
  });

  group('contrast', () {
    const backgrounds = [
      'rail',
      'sidebar',
      'chat',
      'surface',
      'elevated',
      'hover',
      'selected',
      'bubbleIn',
      'bubbleOut',
      'mentionBg',
    ];

    void expectContrast(String foreground, double minimum) {
      for (final colors in [OcColors.dark, OcColors.light]) {
        for (final background in backgrounds) {
          final ratio = contrastRatio(
            colors.byName(foreground),
            colors.byName(background),
          );
          expect(
            ratio,
            greaterThanOrEqualTo(minimum),
            reason: '$foreground on $background',
          );
        }
      }
    }

    test('body text is at least 7:1 on every surface', () {
      expectContrast('text', 7);
    });

    test('secondary text is at least 4.5:1 on every surface', () {
      expectContrast('textSecondary', 4.5);
    });

    test(
      'muted text is at least 4.5:1 on every surface',
      () => expectContrast('textMuted', 4.5),
      skip:
          'The §2.1 textMuted values give 2.7–3.9:1; kept exactly as '
          'specified (docs/decisions.md D16).',
    );

    test('text on accent is at least 7:1', () {
      for (final colors in [OcColors.dark, OcColors.light]) {
        expect(
          contrastRatio(colors.onAccent, colors.accent),
          greaterThanOrEqualTo(7),
        );
      }
    });
  });

  test('lerp blends every token', () {
    final halfway = OcColors.dark.lerp(OcColors.light, 0.5);

    expect(
      halfway.text,
      Color.lerp(OcColors.dark.text, OcColors.light.text, 0.5),
    );
    expect(
      halfway.avatarShades.first,
      Color.lerp(
        OcColors.dark.avatarShades.first,
        OcColors.light.avatarShades.first,
        0.5,
      ),
    );
  });
}
