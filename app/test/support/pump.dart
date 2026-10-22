import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

/// Pumps [child] in a themed app, on a surface of [background].
Future<void> pumpThemed(
  WidgetTester tester,
  Widget child, {
  OcColors colors = OcColors.dark,
  Color Function(OcColors colors)? background,
  Size? surface,
}) async {
  if (surface != null) {
    tester.view.physicalSize = surface;
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
  }
  await tester.pumpWidget(
    MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: buildTheme(colors),
      home: ColoredBox(
        color: (background ?? (c) => c.chat)(colors),
        child: child,
      ),
    ),
  );
}

/// Both themes, for tests and goldens that must hold in each.
const themes = [('dark', OcColors.dark), ('light', OcColors.light)];
