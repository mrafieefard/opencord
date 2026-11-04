import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';

/// A second window's view, for widget tests with more than one view: it
/// lays out at [physicalSize] and draws nothing.
class FakeFlutterView extends Fake implements FlutterView {
  FakeFlutterView(
    this.viewId, {
    required this.display,
    required this.platformDispatcher,
  });

  @override
  final int viewId;

  @override
  final Display display;

  @override
  final PlatformDispatcher platformDispatcher;

  @override
  Size physicalSize = const Size(800, 600);

  @override
  double get devicePixelRatio => 1;

  @override
  ViewConstraints get physicalConstraints =>
      ViewConstraints.tight(physicalSize);

  @override
  ViewPadding get padding => ViewPadding.zero;

  @override
  ViewPadding get viewInsets => ViewPadding.zero;

  @override
  ViewPadding get viewPadding => ViewPadding.zero;

  @override
  ViewPadding get systemGestureInsets => ViewPadding.zero;

  @override
  GestureSettings get gestureSettings => const GestureSettings();

  @override
  List<DisplayFeature> get displayFeatures => const [];

  @override
  DisplayCornerRadii? get displayCornerRadii => null;

  @override
  void render(Scene scene, {Size? size}) {}

  @override
  void updateSemantics(SemanticsUpdate update) {}
}
