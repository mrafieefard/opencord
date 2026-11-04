import 'package:flutter/painting.dart';

/// The media viewer's colours (D86), Telegram's and the same in either
/// theme: everything there sits on a dark, see-through background.
abstract final class OcMediaColors {
  /// `#222222` at 92 %: the desktop or chat behind shows through, dimmed.
  static const background = Color(0xEB222222);

  /// Previous and next's round hover.
  static const navHover = Color(0x47FFFFFF);

  /// The video's bar, Telegram's: black at half, the video showing through.
  static const playerBar = Color(0x80000000);

  /// Picture-in-picture: black behind the video, and a light shade under
  /// its controls so they read over a bright picture.
  static const pipBackground = Color(0xFF000000);
  static const pipShade = Color(0x40000000);

  /// The "Saved to Downloads" note.
  static const note = Color(0xE6000000);

  /// Windows' caption buttons, drawn white with a soft shadow so they
  /// show over a bright photo.
  static const captionGlyph = Color(0xFFFFFFFF);
  static const captionShadow = Color(0x99000000);
  static const captionHover = Color(0x1AFFFFFF);
  static const captionPress = Color(0x0FFFFFFF);

  /// Close hovers inverted, as the main window's does: the app has no hues
  /// (desktop UI plan §1), so not Windows' red.
  static const captionCloseHover = Color(0xFFFFFFFF);
  static const captionClosePress = Color(0xE6FFFFFF);
  static const captionCloseGlyph = Color(0xFF000000);
}
