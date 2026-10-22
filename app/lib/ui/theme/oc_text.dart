import 'package:flutter/painting.dart';

/// The type scale from §2.2. Styles carry no color; widgets add one from
/// the tokens. `label` text is upper-cased by the widget that shows it.
abstract final class OcText {
  static const String fontFamily = 'Inter';
  static const String monoFamily = 'JetBrainsMono';

  static const _base = TextStyle(
    fontFamily: fontFamily,
    leadingDistribution: TextLeadingDistribution.even,
    decoration: TextDecoration.none,
  );

  /// Dialog and settings page titles.
  static final TextStyle title = _base.copyWith(
    fontSize: 17,
    fontWeight: FontWeight.w600,
    height: 1.25,
  );

  /// Server name, channel header, panel titles.
  static final TextStyle header = _base.copyWith(
    fontSize: 15,
    fontWeight: FontWeight.w600,
    height: 1.3,
  );

  /// Messages, row titles.
  static final TextStyle body = _base.copyWith(
    fontSize: 14,
    fontWeight: FontWeight.w400,
    height: 1.35,
  );

  /// Unread row titles, author names.
  static final TextStyle bodyStrong = body.copyWith(
    fontWeight: FontWeight.w600,
  );

  /// Previews, subtitles.
  static final TextStyle small = _base.copyWith(
    fontSize: 12.5,
    fontWeight: FontWeight.w400,
    height: 1.35,
  );

  /// Timestamps in bubbles, "edited".
  static final TextStyle meta = _base.copyWith(
    fontSize: 11,
    fontWeight: FontWeight.w400,
    height: 1.2,
  );

  /// Category and section labels.
  static final TextStyle label = _base.copyWith(
    fontSize: 11.5,
    fontWeight: FontWeight.w700,
    letterSpacing: 0.7,
    height: 1.2,
  );

  /// Code blocks, fingerprints, invite links.
  static final TextStyle mono = _base.copyWith(
    fontFamily: monoFamily,
    fontSize: 12.5,
    fontWeight: FontWeight.w400,
    height: 1.45,
  );
}
