import 'package:flutter/material.dart';

import '../common_props.dart';
import '../view.dart';

/// `badge` (design §12.2): a small pill chip in neutral, accent or danger tone.
final badgeBlock = BlockRenderer(_badge);

Widget _badge(BlockBuild b) {
  final props = b.props;
  final text = props['text'] as String? ?? '';
  final tone = props['tone'] as String? ?? 'neutral';
  final theme = b.theme;

  final (Color bg, Color fg, Border? border) = switch (tone) {
    'accent' => (theme.colors['accent']!, theme.colors['on_accent']!, null),
    'danger' => (theme.colors['danger']!, theme.colors['on_accent']!, null),
    _ => (
      theme.colors['surface']!,
      theme.colors['text_secondary']!,
      Border.all(color: theme.colors['border']!, width: 1),
    ),
  };

  return Container(
    padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
    decoration: BoxDecoration(
      color: bg,
      borderRadius: BorderRadius.circular(10),
      border: border,
    ),
    child: Text(
      text,
      style: TextStyle(
        fontFamily: theme.text.fontFamily ?? 'Inter',
        fontSize: 11,
        fontWeight: FontWeight.w600,
        color: fg,
        height: 1.2,
      ),
    ),
  );
}

/// `progress` (design §12.2): bar or ring progress indicator, indeterminate
/// when [value] is absent.
final progressBlock = BlockRenderer(_progress);

Widget _progress(BlockBuild b) {
  final props = b.props;
  final value = numberProp(props['value']);
  final style = props['style'] as String? ?? 'bar';
  final theme = b.theme;
  final accent = theme.colors['accent']!;
  final bg = theme.colors['surface']!;

  if (style == 'ring') {
    return SizedBox.square(
      dimension: 16,
      child: CircularProgressIndicator(
        value: value,
        strokeWidth: 2,
        color: accent,
        backgroundColor: bg,
      ),
    );
  }

  return LinearProgressIndicator(
    value: value,
    minHeight: 4,
    color: accent,
    backgroundColor: bg,
    borderRadius: BorderRadius.circular(2),
  );
}

/// `spinner` (design §12.2): the app's spinner look drawn locally.
final spinnerBlock = BlockRenderer(_spinner);

Widget _spinner(BlockBuild b) {
  final size = numberProp(b.props['size']) ?? 16.0;
  final color = b.theme.colors['text']!;

  return SizedBox.square(
    dimension: size,
    child: CircularProgressIndicator(strokeWidth: 2, color: color),
  );
}
