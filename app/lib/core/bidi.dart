import 'package:flutter/painting.dart';

/// Hebrew, Arabic, Syriac, Thaana, N'Ko and the other right-to-left
/// scripts, with their presentation forms.
bool _rtl(int rune) =>
    (rune >= 0x0590 && rune <= 0x08FF) ||
    (rune >= 0xFB1D && rune <= 0xFDFF) ||
    (rune >= 0xFE70 && rune <= 0xFEFF) ||
    (rune >= 0x10800 && rune <= 0x10FFF) ||
    (rune >= 0x1E800 && rune <= 0x1EFFF);

final _letter = RegExp(r'\p{L}', unicode: true);

/// The direction of [text] by its first letter, as Telegram sets a message
/// box's: right-to-left from the first Persian, Arabic or Hebrew letter,
/// left-to-right from the first other one. Null when there is no letter
/// yet (empty, or only digits, spaces and symbols), to keep the ambient
/// direction.
TextDirection? textDirectionOf(String text) {
  for (final rune in text.runes) {
    if (_rtl(rune)) return TextDirection.rtl;
    if (_letter.hasMatch(String.fromCharCode(rune))) return TextDirection.ltr;
  }
  return null;
}
