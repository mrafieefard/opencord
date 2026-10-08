import 'package:flutter/services.dart';

/// A hotkey's key and modifiers, written as the XDG shortcuts specification
/// writes them (`CTRL+SHIFT+m`, `LOGO+F12`, `grave`): the key is an xkb
/// keysym name. The same keys as the core accepts.
class Accelerator {
  const Accelerator({
    required this.key,
    this.ctrl = false,
    this.alt = false,
    this.shift = false,
    this.logo = false,
  });

  final LogicalKeyboardKey key;
  final bool ctrl;
  final bool alt;
  final bool shift;
  final bool logo;

  /// Null when [text] names a key or modifier a hotkey cannot use.
  static Accelerator? parse(String text) {
    final parts = text.split('+').map((part) => part.trim()).toList();
    final key = _key(parts.removeLast());
    if (key == null) return null;
    var (ctrl, alt, shift, logo) = (false, false, false, false);
    for (final modifier in parts) {
      switch (modifier.toUpperCase()) {
        case 'CTRL':
          ctrl = true;
        case 'ALT':
          alt = true;
        case 'SHIFT':
          shift = true;
        case 'LOGO':
          logo = true;
        default:
          return null;
      }
    }
    return Accelerator(
      key: key,
      ctrl: ctrl,
      alt: alt,
      shift: shift,
      logo: logo,
    );
  }

  /// Whether the modifiers it needs are held (others may be too).
  bool modifiersHeld(HardwareKeyboard keyboard) =>
      (!ctrl || keyboard.isControlPressed) &&
      (!alt || keyboard.isAltPressed) &&
      (!shift || keyboard.isShiftPressed) &&
      (!logo || keyboard.isMetaPressed);
}

LogicalKeyboardKey? _key(String name) {
  if (name.length == 1) {
    final code = name.codeUnitAt(0);
    final letterOrDigit =
        (code >= 0x61 && code <= 0x7a) || (code >= 0x30 && code <= 0x39);
    return letterOrDigit ? LogicalKeyboardKey.findKeyByKeyId(code) : null;
  }
  if (RegExp(r'^F(\d{1,2})$').firstMatch(name) case final match?) {
    final number = int.parse(match.group(1)!);
    if (number < 1 || number > 24) return null;
    return LogicalKeyboardKey.findKeyByKeyId(
      LogicalKeyboardKey.f1.keyId + number - 1,
    );
  }
  if (RegExp(r'^KP_(\d)$').firstMatch(name) case final match?) {
    return LogicalKeyboardKey.findKeyByKeyId(
      LogicalKeyboardKey.numpad0.keyId + int.parse(match.group(1)!),
    );
  }
  return switch (name) {
    'space' => LogicalKeyboardKey.space,
    'apostrophe' => LogicalKeyboardKey.quote,
    'comma' => LogicalKeyboardKey.comma,
    'minus' => LogicalKeyboardKey.minus,
    'period' => LogicalKeyboardKey.period,
    'slash' => LogicalKeyboardKey.slash,
    'semicolon' => LogicalKeyboardKey.semicolon,
    'equal' => LogicalKeyboardKey.equal,
    'bracketleft' => LogicalKeyboardKey.bracketLeft,
    'backslash' => LogicalKeyboardKey.backslash,
    'bracketright' => LogicalKeyboardKey.bracketRight,
    'grave' => LogicalKeyboardKey.backquote,
    'Tab' => LogicalKeyboardKey.tab,
    'Pause' => LogicalKeyboardKey.pause,
    'Scroll_Lock' => LogicalKeyboardKey.scrollLock,
    'Home' => LogicalKeyboardKey.home,
    'Prior' => LogicalKeyboardKey.pageUp,
    'Next' => LogicalKeyboardKey.pageDown,
    'End' => LogicalKeyboardKey.end,
    'Insert' => LogicalKeyboardKey.insert,
    'Delete' => LogicalKeyboardKey.delete,
    _ => null,
  };
}
