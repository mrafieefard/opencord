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

  /// For what leaves on its own.
  static const Curve curveIn = Curves.easeIn;

  /// How long the notch stays as it is after the pane button is clicked, so
  /// the button is seen to go first (§5.9).
  static const Duration notchDelay = Duration(milliseconds: 50);

  /// How long the rail's popover waits for the pointer to come back (§5.2).
  static const Duration railPopWait = Duration(milliseconds: 90);

  bool get reduced => _reduced;

  Duration _maybe(int milliseconds) =>
      _reduced ? Duration.zero : Duration(milliseconds: milliseconds);

  /// The rail popover's fade and, leaving, its scale.
  Duration get railPop => _maybe(80);

  /// Hover and press color changes.
  Duration get hover => _maybe(120);

  /// Press scale.
  Duration get press => _maybe(80);

  /// What leaves on its own: the voice controls, the pane button.
  Duration get exit => _maybe(130);

  /// A fade in.
  Duration get fade => _maybe(120);

  /// A small thing's fade in: the pane button coming back.
  Duration get fadeQuick => _maybe(100);

  /// Small rotations, such as a dropdown's arrow.
  Duration get turn => _maybe(140);

  /// Server icon morph, category arrow, badge appearing.
  Duration get morph => _maybe(150);

  /// Dialog fade and scale.
  Duration get dialog => _maybe(120);

  /// Menu fade.
  Duration get menu => _maybe(90);

  /// Message arrival and removal fade.
  Duration get message => _maybe(100);

  /// Settings' save bar rising from the bottom, as Discord's does.
  Duration get saveBar => _maybe(200);
}
