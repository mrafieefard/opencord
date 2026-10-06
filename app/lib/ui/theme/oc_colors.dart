import 'package:flutter/material.dart';

/// The color tokens from the desktop UI plan §2.1. Every color the app
/// paints comes from here; widgets read them with `context.oc`.
@immutable
class OcColors extends ThemeExtension<OcColors> {
  const OcColors({
    required this.rail,
    required this.sidebar,
    required this.chat,
    required this.surface,
    required this.elevated,
    required this.hover,
    required this.selected,
    required this.border,
    required this.text,
    required this.textSecondary,
    required this.textMuted,
    required this.accent,
    required this.onAccent,
    required this.bubbleIn,
    required this.bubbleOut,
    required this.mentionBg,
    required this.scrim,
    required this.avatarShades,
  });

  static const dark = OcColors(
    rail: Color(0xFF0A0A0B),
    sidebar: Color(0xFF121214),
    chat: Color(0xFF0E0E10),
    surface: Color(0xFF18181B),
    elevated: Color(0xFF1F1F23),
    hover: Color(0xFF1C1C20),
    selected: Color(0xFF27272C),
    border: Color(0xFF222226),
    text: Color(0xFFEDEDEF),
    textSecondary: Color(0xFFA1A1A8),
    textMuted: Color(0xFF6E6E76),
    accent: Color(0xFFEDEDEF),
    onAccent: Color(0xFF0E0E10),
    bubbleIn: Color(0xFF1A1A1E),
    bubbleOut: Color(0xFF2A2A30),
    mentionBg: Color(0xFF26262B),
    scrim: Color(0xB3000000),
    avatarShades: [
      Color(0xFF2B2B30),
      Color(0xFF34343A),
      Color(0xFF3E3E45),
      Color(0xFF494951),
      Color(0xFF55555D),
    ],
  );

  static const light = OcColors(
    rail: Color(0xFFE9E9EC),
    sidebar: Color(0xFFFFFFFF),
    chat: Color(0xFFF3F3F5),
    surface: Color(0xFFFFFFFF),
    elevated: Color(0xFFFFFFFF),
    hover: Color(0xFFF2F2F4),
    selected: Color(0xFFE7E7EA),
    border: Color(0xFFE3E3E7),
    text: Color(0xFF111114),
    textSecondary: Color(0xFF55555C),
    textMuted: Color(0xFF8A8A92),
    accent: Color(0xFF111114),
    onAccent: Color(0xFFFFFFFF),
    bubbleIn: Color(0xFFFFFFFF),
    bubbleOut: Color(0xFFE4E4E8),
    mentionBg: Color(0xFFE2E2E6),
    scrim: Color(0x66000000),
    avatarShades: [
      Color(0xFFD9D9DE),
      Color(0xFFCDCDD3),
      Color(0xFFC1C1C8),
      Color(0xFFE3E3E7),
      Color(0xFFB6B6BE),
    ],
  );

  /// Server rail background.
  final Color rail;

  /// Channel sidebar, headers, member panel, composer bar.
  final Color sidebar;

  /// Message area background.
  final Color chat;

  /// Cards, video tiles, settings sections.
  final Color surface;

  /// Dialogs, menus, popovers.
  final Color elevated;

  /// Hovered rows, input fills.
  final Color hover;

  /// Selected rows, hovered buttons, pills.
  final Color selected;

  /// 1 px dividers and outlines.
  final Color border;

  /// Primary text and icons.
  final Color text;

  /// Secondary text, inactive icons.
  final Color textSecondary;

  /// Timestamps, hints, previews, section labels.
  final Color textMuted;

  /// Inverted emphasis background (same as [text]).
  final Color accent;

  /// Foreground on [accent].
  final Color onAccent;

  /// Incoming message bubble.
  final Color bubbleIn;

  /// Own message bubble.
  final Color bubbleOut;

  /// Mention and inline-code highlight.
  final Color mentionBg;

  /// Dialog barrier.
  final Color scrim;

  /// Avatar backgrounds, picked by a hash of the user or server id.
  final List<Color> avatarShades;

  /// The grey an id always gets as its avatar background.
  Color avatarShade(Object id) =>
      avatarShades[stableHash(id.toString()) % avatarShades.length];

  /// Every single-color token by name.
  Map<String, Color> get all => {
    'rail': rail,
    'sidebar': sidebar,
    'chat': chat,
    'surface': surface,
    'elevated': elevated,
    'hover': hover,
    'selected': selected,
    'border': border,
    'text': text,
    'textSecondary': textSecondary,
    'textMuted': textMuted,
    'accent': accent,
    'onAccent': onAccent,
    'bubbleIn': bubbleIn,
    'bubbleOut': bubbleOut,
    'mentionBg': mentionBg,
    'scrim': scrim,
  };

  Color byName(String name) =>
      all[name] ?? (throw ArgumentError.value(name, 'name', 'unknown token'));

  @override
  OcColors copyWith({
    Color? rail,
    Color? sidebar,
    Color? chat,
    Color? surface,
    Color? elevated,
    Color? hover,
    Color? selected,
    Color? border,
    Color? text,
    Color? textSecondary,
    Color? textMuted,
    Color? accent,
    Color? onAccent,
    Color? bubbleIn,
    Color? bubbleOut,
    Color? mentionBg,
    Color? scrim,
    List<Color>? avatarShades,
  }) {
    return OcColors(
      rail: rail ?? this.rail,
      sidebar: sidebar ?? this.sidebar,
      chat: chat ?? this.chat,
      surface: surface ?? this.surface,
      elevated: elevated ?? this.elevated,
      hover: hover ?? this.hover,
      selected: selected ?? this.selected,
      border: border ?? this.border,
      text: text ?? this.text,
      textSecondary: textSecondary ?? this.textSecondary,
      textMuted: textMuted ?? this.textMuted,
      accent: accent ?? this.accent,
      onAccent: onAccent ?? this.onAccent,
      bubbleIn: bubbleIn ?? this.bubbleIn,
      bubbleOut: bubbleOut ?? this.bubbleOut,
      mentionBg: mentionBg ?? this.mentionBg,
      scrim: scrim ?? this.scrim,
      avatarShades: avatarShades ?? this.avatarShades,
    );
  }

  @override
  OcColors lerp(OcColors? other, double t) {
    if (other == null) return this;
    Color mix(Color a, Color b) => Color.lerp(a, b, t)!;
    return OcColors(
      rail: mix(rail, other.rail),
      sidebar: mix(sidebar, other.sidebar),
      chat: mix(chat, other.chat),
      surface: mix(surface, other.surface),
      elevated: mix(elevated, other.elevated),
      hover: mix(hover, other.hover),
      selected: mix(selected, other.selected),
      border: mix(border, other.border),
      text: mix(text, other.text),
      textSecondary: mix(textSecondary, other.textSecondary),
      textMuted: mix(textMuted, other.textMuted),
      accent: mix(accent, other.accent),
      onAccent: mix(onAccent, other.onAccent),
      bubbleIn: mix(bubbleIn, other.bubbleIn),
      bubbleOut: mix(bubbleOut, other.bubbleOut),
      mentionBg: mix(mentionBg, other.mentionBg),
      scrim: mix(scrim, other.scrim),
      avatarShades: [
        for (var i = 0; i < avatarShades.length; i++)
          mix(avatarShades[i], other.avatarShades[i]),
      ],
    );
  }
}

/// FNV-1a over the UTF-16 code units, so the result is the same on every
/// run and platform (unlike `String.hashCode`).
int stableHash(String value) {
  var hash = 0x811c9dc5;
  for (final unit in value.codeUnits) {
    hash = ((hash ^ unit) * 0x01000193) & 0xffffffff;
  }
  return hash;
}

extension OcColorsContext on BuildContext {
  /// The color tokens of the current theme.
  OcColors get oc => Theme.of(this).extension<OcColors>()!;
}
