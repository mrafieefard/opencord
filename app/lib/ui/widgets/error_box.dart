import 'package:flutter/widgets.dart';

ErrorWidgetBuilder? _standard;

/// Keeps a widget that failed to build to one line: Flutter's error box
/// fills the room it is given, which in a list is 100,000 px, so one bad
/// message would push everything else out of sight.
void limitErrorBoxes() {
  final standard = _standard ??= ErrorWidget.builder;
  ErrorWidget.builder = (details) =>
      LimitedBox(maxHeight: 48, child: standard(details));
}
