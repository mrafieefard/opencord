import 'dart:math' as math;
import 'dart:ui' show Brightness;

import 'package:flutter/painting.dart';

/// The theme's colour names a plugin may use: the same list as the core's
/// checks (`opencord_plugins::blocks::COLORS`; a test compares them).
const blockColorNames = [
  'text',
  'text_secondary',
  'text_muted',
  'background',
  'surface',
  'elevated',
  'border',
  'accent',
  'on_accent',
  'selected',
  'hover',
  'danger',
];

/// The viewer's look, which blocks use unless the plugin sets colours. The
/// app builds it from its own theme (design §12.2).
class BlocksTheme {
  BlocksTheme({
    required this.brightness,
    required Map<String, Color> colors,
    required this.text,
    this.radius = 8,
    this.minTouchSize = 0,
    Map<String, List<BoxShadow>>? shadows,
  }) : colors = Map.unmodifiable(colors),
       shadows = Map.unmodifiable(shadows ?? defaultShadows) {
    assert(
      blockColorNames.every(colors.containsKey),
      'BlocksTheme needs every colour of blockColorNames; missing: '
      '${blockColorNames.where((name) => !colors.containsKey(name)).join(', ')}',
    );
  }

  /// Shadows for the `shadow` prop's `small`, `medium` and `large`.
  static const defaultShadows = {
    'small': [
      BoxShadow(color: Color(0x29000000), blurRadius: 4, offset: Offset(0, 1)),
    ],
    'medium': [
      BoxShadow(color: Color(0x33000000), blurRadius: 12, offset: Offset(0, 4)),
    ],
    'large': [
      BoxShadow(
        color: Color(0x40000000),
        blurRadius: 28,
        offset: Offset(0, 12),
      ),
    ],
  };

  final Brightness brightness;

  /// Every name of [blockColorNames].
  final Map<String, Color> colors;

  /// Text as blocks draw it: the family, size and `text` colour.
  final TextStyle text;

  /// The corner radius blocks use for their own shapes.
  final double radius;

  /// The least height and width a control's tap target takes, in logical
  /// pixels; what the control draws keeps its size, centred in it. 0 on the
  /// desktop (a mouse); 44 on a phone, for a finger.
  final double minTouchSize;

  final Map<String, List<BoxShadow>> shadows;

  /// What a block that can't be drawn says, in the theme's `danger`.
  TextStyle get errorText => text.copyWith(color: colors['danger']);

  /// A colour name, `#rrggbb` or `#rrggbbaa` (alpha last, as in CSS); null
  /// for anything else, gradients included.
  Color? resolveColor(Object? value) {
    if (value is! String) return null;
    if (!value.startsWith('#')) return colors[value];
    final hex = value.substring(1);
    if (hex.length != 6 && hex.length != 8) return null;
    final rgba = int.tryParse(hex, radix: 16);
    if (rgba == null || hex.contains(RegExp('[^0-9a-fA-F]'))) return null;
    return hex.length == 6
        ? Color(0xFF000000 | rgba)
        : Color(((rgba & 0xFF) << 24) | (rgba >> 8));
  }

  /// A gradient `{from, to, angle?}`; null for anything else. The angle
  /// follows CSS: degrees, 0 towards the top, 90 towards the right, 180
  /// (towards the bottom) when left out.
  Gradient? resolveGradient(Object? value) {
    if (value is! Map<String, Object?>) return null;
    final from = resolveColor(value['from']);
    final to = resolveColor(value['to']);
    final angle = value['angle'] ?? 180;
    if (from == null || to == null || angle is! num || !angle.isFinite) {
      return null;
    }
    final radians = angle * math.pi / 180;
    double snap(double side) => side.abs() < 1e-9 ? 0 : side;
    final x = snap(math.sin(radians));
    final y = snap(-math.cos(radians));
    final reach = math.max(x.abs(), y.abs());
    final end = Alignment(x / reach, y / reach);
    return LinearGradient(begin: -end, end: end, colors: [from, to]);
  }
}
