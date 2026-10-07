import 'package:flutter/widgets.dart';

/// Durations and curves from §2.4. With reduce motion on (the OS setting or
/// the app's own, both surface as `MediaQuery.disableAnimations`), every
/// non-essential animation takes no time.
class OcMotion {
  const OcMotion._(this._reduced);

  factory OcMotion.of(BuildContext context) =>
      OcMotion._(MediaQuery.disableAnimationsOf(context));

  final bool _reduced;

  static const Curve curve = Curves.easeOut;

  Duration _maybe(int milliseconds) =>
      _reduced ? Duration.zero : Duration(milliseconds: milliseconds);

  /// Hover and press color changes.
  Duration get hover => _maybe(120);

  /// Server icon morph, category arrow, badge appearing.
  Duration get morph => _maybe(150);

  /// Dialog fade and scale.
  Duration get dialog => _maybe(120);

  /// Menu fade.
  Duration get menu => _maybe(90);

  /// Message arrival and removal fade.
  Duration get message => _maybe(100);

  /// A mouse wheel notch gliding through the chat (§15).
  Duration get wheel => _maybe(140);
}
