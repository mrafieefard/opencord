import 'package:flutter/services.dart';

/// A hotkey's key and modifiers, written as the XDG shortcuts specification
/// writes them (`CTRL+SHIFT+m`, `LOGO+F12`, `grave`, `Control_R`): the key
/// is an xkb keysym name. The same keys as the core accepts.
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
    final key = _keyNamed(parts.removeLast());
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

  /// Whether a hotkey can use [key].
  static bool canUse(LogicalKeyboardKey key) => _nameOf(key) != null;

  /// Whether the modifiers it needs are held (others may be too).
  bool modifiersHeld(HardwareKeyboard keyboard) =>
      (!ctrl || keyboard.isControlPressed) &&
      (!alt || keyboard.isAltPressed) &&
      (!shift || keyboard.isShiftPressed) &&
      (!logo || keyboard.isMetaPressed);

  /// As the core and the XDG specification write it.
  @override
  String toString() => [
    if (ctrl) 'CTRL',
    if (alt) 'ALT',
    if (shift) 'SHIFT',
    if (logo) 'LOGO',
    _nameOf(key) ?? key.keyLabel,
  ].join('+');

  /// For people: `Ctrl + Shift + M`, `Right Ctrl`, `` ` ``.
  String label({bool mac = false}) => [
    if (ctrl) mac ? '⌃' : 'Ctrl',
    if (alt) mac ? '⌥' : 'Alt',
    if (shift) mac ? '⇧' : 'Shift',
    if (logo) mac ? '⌘' : 'Super',
    keyLabel(key, mac: mac),
  ].join(mac ? '' : ' + ');

  @override
  bool operator ==(Object other) =>
      other is Accelerator &&
      other.key == key &&
      other.ctrl == ctrl &&
      other.alt == alt &&
      other.shift == shift &&
      other.logo == logo;

  @override
  int get hashCode => Object.hash(key, ctrl, alt, shift, logo);
}

/// A key's name for people.
String keyLabel(LogicalKeyboardKey key, {bool mac = false}) {
  if (_numpadDigit(key) case final digit?) return 'Numpad $digit';
  return switch (key) {
    LogicalKeyboardKey.controlLeft => mac ? 'Left ⌃' : 'Left Ctrl',
    LogicalKeyboardKey.controlRight => mac ? 'Right ⌃' : 'Right Ctrl',
    LogicalKeyboardKey.altLeft => mac ? 'Left ⌥' : 'Left Alt',
    LogicalKeyboardKey.altRight => mac ? 'Right ⌥' : 'Right Alt',
    LogicalKeyboardKey.shiftLeft => 'Left Shift',
    LogicalKeyboardKey.shiftRight => 'Right Shift',
    LogicalKeyboardKey.metaLeft => mac ? 'Left ⌘' : 'Left Super',
    LogicalKeyboardKey.metaRight => mac ? 'Right ⌘' : 'Right Super',
    LogicalKeyboardKey.space => 'Space',
    LogicalKeyboardKey.backquote => 'Backtick',
    LogicalKeyboardKey.capsLock => 'Caps Lock',
    LogicalKeyboardKey.scrollLock => 'Scroll Lock',
    LogicalKeyboardKey.numLock => 'Num Lock',
    LogicalKeyboardKey.pageUp => 'Page Up',
    LogicalKeyboardKey.pageDown => 'Page Down',
    LogicalKeyboardKey.backspace => 'Backspace',
    LogicalKeyboardKey.enter => 'Enter',
    LogicalKeyboardKey.escape => 'Esc',
    LogicalKeyboardKey.printScreen => 'Print Screen',
    LogicalKeyboardKey.contextMenu => 'Menu',
    LogicalKeyboardKey.arrowLeft => '←',
    LogicalKeyboardKey.arrowUp => '↑',
    LogicalKeyboardKey.arrowRight => '→',
    LogicalKeyboardKey.arrowDown => '↓',
    LogicalKeyboardKey.numpadEnter => 'Numpad Enter',
    LogicalKeyboardKey.numpadMultiply => 'Numpad *',
    LogicalKeyboardKey.numpadAdd => 'Numpad +',
    LogicalKeyboardKey.numpadSubtract => 'Numpad -',
    LogicalKeyboardKey.numpadDecimal => 'Numpad .',
    LogicalKeyboardKey.numpadDivide => 'Numpad /',
    _ => key.keyLabel.length == 1 ? key.keyLabel.toUpperCase() : key.keyLabel,
  };
}

/// Keys other than letters, digits, F1–F24 and the keypad's digits, by
/// keysym name.
const _named = <String, LogicalKeyboardKey>{
  'space': LogicalKeyboardKey.space,
  'apostrophe': LogicalKeyboardKey.quote,
  'comma': LogicalKeyboardKey.comma,
  'minus': LogicalKeyboardKey.minus,
  'period': LogicalKeyboardKey.period,
  'slash': LogicalKeyboardKey.slash,
  'semicolon': LogicalKeyboardKey.semicolon,
  'equal': LogicalKeyboardKey.equal,
  'bracketleft': LogicalKeyboardKey.bracketLeft,
  'backslash': LogicalKeyboardKey.backslash,
  'bracketright': LogicalKeyboardKey.bracketRight,
  'grave': LogicalKeyboardKey.backquote,
  'Tab': LogicalKeyboardKey.tab,
  'Pause': LogicalKeyboardKey.pause,
  'Scroll_Lock': LogicalKeyboardKey.scrollLock,
  'Home': LogicalKeyboardKey.home,
  'Prior': LogicalKeyboardKey.pageUp,
  'Next': LogicalKeyboardKey.pageDown,
  'End': LogicalKeyboardKey.end,
  'Insert': LogicalKeyboardKey.insert,
  'Delete': LogicalKeyboardKey.delete,
  'BackSpace': LogicalKeyboardKey.backspace,
  'Return': LogicalKeyboardKey.enter,
  'Escape': LogicalKeyboardKey.escape,
  'Left': LogicalKeyboardKey.arrowLeft,
  'Up': LogicalKeyboardKey.arrowUp,
  'Right': LogicalKeyboardKey.arrowRight,
  'Down': LogicalKeyboardKey.arrowDown,
  'Print': LogicalKeyboardKey.printScreen,
  'Menu': LogicalKeyboardKey.contextMenu,
  'Num_Lock': LogicalKeyboardKey.numLock,
  'KP_Enter': LogicalKeyboardKey.numpadEnter,
  'KP_Multiply': LogicalKeyboardKey.numpadMultiply,
  'KP_Add': LogicalKeyboardKey.numpadAdd,
  'KP_Subtract': LogicalKeyboardKey.numpadSubtract,
  'KP_Decimal': LogicalKeyboardKey.numpadDecimal,
  'KP_Divide': LogicalKeyboardKey.numpadDivide,
  'Shift_L': LogicalKeyboardKey.shiftLeft,
  'Shift_R': LogicalKeyboardKey.shiftRight,
  'Control_L': LogicalKeyboardKey.controlLeft,
  'Control_R': LogicalKeyboardKey.controlRight,
  'Caps_Lock': LogicalKeyboardKey.capsLock,
  'Alt_L': LogicalKeyboardKey.altLeft,
  'Alt_R': LogicalKeyboardKey.altRight,
  'Super_L': LogicalKeyboardKey.metaLeft,
  'Super_R': LogicalKeyboardKey.metaRight,
};

final _names = {
  for (final MapEntry(:key, :value) in _named.entries) value: key,
};

int? _numpadDigit(LogicalKeyboardKey key) {
  final digit = key.keyId - LogicalKeyboardKey.numpad0.keyId;
  return digit >= 0 && digit <= 9 ? digit : null;
}

LogicalKeyboardKey? _keyNamed(String name) {
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
  return _named[name];
}

String? _nameOf(LogicalKeyboardKey key) {
  final id = key.keyId;
  if ((id >= 0x61 && id <= 0x7a) || (id >= 0x30 && id <= 0x39)) {
    return String.fromCharCode(id);
  }
  final function = id - LogicalKeyboardKey.f1.keyId;
  if (function >= 0 && function < 24) return 'F${function + 1}';
  if (_numpadDigit(key) case final digit?) return 'KP_$digit';
  return _names[key];
}
