import 'package:flutter/widgets.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

/// Builds a [BlocksTheme] from Opencord's [OcColors], using Inter for typography,
/// mapping `danger` to `recording` and `background` to `chat`.
BlocksTheme buildBlocksTheme(
  OcColors colors, {
  required Brightness brightness,
}) {
  return BlocksTheme(
    brightness: brightness,
    colors: {
      'text': colors.text,
      'text_secondary': colors.textSecondary,
      'text_muted': colors.textMuted,
      'background': colors.chat,
      'surface': colors.surface,
      'elevated': colors.elevated,
      'border': colors.border,
      'accent': colors.accent,
      'on_accent': colors.onAccent,
      'selected': colors.selected,
      'hover': colors.hover,
      'danger': colors.recording,
    },
    text: OcText.body.copyWith(
      fontFamily: 'Inter',
      color: colors.text,
      fontSize: 14,
    ),
    radius: 8,
  );
}
