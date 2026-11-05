import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/shell_splitter.dart';

import '../../support/app.dart';

class CountingStore implements KeyValueStore {
  final Map<String, String> _values = {};
  int writeCount = 0;

  @override
  String? read(String key) => _values[key];

  @override
  void write(String key, String? value) {
    writeCount++;
    if (value == null) {
      _values.remove(key);
    } else {
      _values[key] = value;
    }
  }
}

Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 20; i++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

void main() {
  testWidgets('drag_resizes_live_without_animation', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await _settle(tester);

    final leftRectBefore = tester.getRect(find.byKey(ShellKeys.left));
    expect(leftRectBefore.width, 288.0);
    final mainRectBefore = tester.getRect(find.byKey(ShellKeys.main));

    final splitter = find.byKey(ShellKeys.splitter);
    expect(splitter, findsOneWidget);

    final gesture = await tester.startGesture(tester.getCenter(splitter));
    await gesture.moveBy(const Offset(30, 0));
    // Pump 1 single frame — must be at 30 immediately without animation
    await tester.pump();

    final leftRectAfter = tester.getRect(find.byKey(ShellKeys.left));
    expect(leftRectAfter.width, 318.0);

    expect(tester.getRect(find.byKey(ShellKeys.head)).width, 318.0);
    expect(tester.getRect(find.byKey(ShellKeys.channels)).width, 318.0);
    expect(tester.getRect(find.byKey(ShellKeys.me)).width, 318.0);

    final mainRectAfter = tester.getRect(find.byKey(ShellKeys.main));
    expect(mainRectAfter.width, mainRectBefore.width - 30);

    await gesture.up();
    await tester.pump();
    await app.dispose(tester);
  });

  testWidgets('drag_stops_at_the_limits', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await _settle(tester);

    final splitter = find.byKey(ShellKeys.splitter);
    final gesture = await tester.startGesture(tester.getCenter(splitter));

    // Drag left past the 232 minimum
    await gesture.moveBy(const Offset(-200, 0));
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 232.0);

    // Drag right past the 420 maximum
    await gesture.moveBy(const Offset(400, 0));
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 420.0);

    await gesture.up();
    await tester.pump();
    await app.dispose(tester);
  });

  testWidgets('written_once_on_release', (tester) async {
    final store = CountingStore();
    final app = await MockApp.pump(
      tester,
      size: const Size(1280, 800),
      store: store,
    );
    await _settle(tester);

    final initialWrites = store.writeCount;

    final splitter = find.byKey(ShellKeys.splitter);
    final gesture = await tester.startGesture(tester.getCenter(splitter));

    await gesture.moveBy(const Offset(10, 0));
    await tester.pump();
    await gesture.moveBy(const Offset(10, 0));
    await tester.pump();
    await gesture.moveBy(const Offset(10, 0));
    await tester.pump();

    // No writes while dragging
    expect(store.writeCount, initialWrites);

    // Release writes once
    await gesture.up();
    await tester.pump();
    expect(store.writeCount, initialWrites + 1);
    expect(store.read(sidebarWidthKey), '318.0');

    await app.dispose(tester);
  });

  testWidgets('remembered_across_launches', (tester) async {
    final store = MemoryKeyValueStore();
    store.write(sidebarWidthKey, '334.0');

    final app = await MockApp.pump(
      tester,
      size: const Size(1280, 800),
      store: store,
    );
    await _settle(tester);

    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 334.0);
    await app.dispose(tester);
  });

  testWidgets('double_click_resets_on_the_spring', (tester) async {
    final store = MemoryKeyValueStore();
    store.write(sidebarWidthKey, '350.0');

    final app = await MockApp.pump(
      tester,
      size: const Size(1280, 800),
      store: store,
    );
    await _settle(tester);
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 350.0);

    final splitter = find.byKey(ShellKeys.splitter);

    // Double tap splitter
    await tester.tap(splitter);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tap(splitter);
    await tester.pump();

    // Mid-spring: width is moving from 350 towards 288
    await tester.pump(const Duration(milliseconds: 40));
    final midWidth = tester.getRect(find.byKey(ShellKeys.left)).width;
    expect(midWidth, lessThan(350.0));
    expect(midWidth, greaterThan(288.0));

    // Settles at 288.0
    await _settle(tester);
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 288.0);
    expect(store.read(sidebarWidthKey), isNull);

    await app.dispose(tester);
  });

  testWidgets('window_narrower_pulls_it_in_and_wider_restores_it', (
    tester,
  ) async {
    final store = MemoryKeyValueStore();
    store.write(sidebarWidthKey, '400.0');

    final app = await MockApp.pump(
      tester,
      size: const Size(1280, 800),
      store: store,
    );
    await _settle(tester);
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 400.0);

    // Narrow window to 800: max is 800 - 62 - 32 - 0 - 380 = 326.0
    tester.view.physicalSize = const Size(800, 800);
    await tester.pump();

    // Springs to 326.0
    await _settle(tester);
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 326.0);
    // Setting in store is still 400.0!
    expect(store.read(sidebarWidthKey), '400.0');

    // Widen back to 1280: restores to 400.0
    tester.view.physicalSize = const Size(1280, 800);
    await tester.pump();
    await _settle(tester);
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 400.0);

    await app.dispose(tester);
  });

  testWidgets('keys_step_16_and_home_resets', (tester) async {
    final store = MemoryKeyValueStore();
    final app = await MockApp.pump(
      tester,
      size: const Size(1280, 800),
      store: store,
    );
    await _settle(tester);

    final sliderSemantics = find.bySemanticsLabel('Resize the channel list');
    expect(sliderSemantics, findsOneWidget);

    // Focus the splitter
    final focusable = find.descendant(
      of: find.byKey(ShellKeys.splitter),
      matching: find.byType(Focus),
    );
    expect(focusable, findsOneWidget);
    tester.widget<Focus>(focusable).focusNode!.requestFocus();
    await tester.pump();

    // Step right +16
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 304.0);
    expect(store.read(sidebarWidthKey), '304.0');

    // Step left -16
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 288.0);
    expect(store.read(sidebarWidthKey), '288.0');

    // Step right twice to 320
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 320.0);

    // Home resets to 288
    await tester.sendKeyEvent(LogicalKeyboardKey.home);
    await _settle(tester);
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 288.0);
    expect(store.read(sidebarWidthKey), isNull);

    await app.dispose(tester);
  });

  testWidgets('pill_appears_on_hover', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await _settle(tester);

    final splitter = find.byKey(ShellKeys.splitter);
    final pillOpacity = find.descendant(
      of: splitter,
      matching: find.byType(AnimatedOpacity),
    );
    expect(
      tester.renderObject<RenderAnimatedOpacity>(pillOpacity).opacity.value,
      0.0,
    );

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(splitter));
    await tester.pump();

    // At 60ms opacity is fading in
    await tester.pump(const Duration(milliseconds: 60));
    final midOpacity = tester
        .renderObject<RenderAnimatedOpacity>(pillOpacity)
        .opacity
        .value;
    expect(midOpacity, greaterThan(0.0));
    expect(midOpacity, lessThan(1.0));

    // Full 120ms
    await tester.pump(const Duration(milliseconds: 80));
    expect(
      tester.renderObject<RenderAnimatedOpacity>(pillOpacity).opacity.value,
      1.0,
    );

    final pillContainer = find.descendant(
      of: splitter,
      matching: find.byType(AnimatedContainer),
    );
    final size = tester.getSize(pillContainer);
    expect(size.width, 3.0);
    expect(size.height, 40.0);

    // Move away
    await mouse.moveTo(Offset.zero);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 150));
    expect(
      tester.renderObject<RenderAnimatedOpacity>(pillOpacity).opacity.value,
      0.0,
    );

    await mouse.removePointer();
    await app.dispose(tester);
  });

  testWidgets('no_splitter_in_compact', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(750, 800));
    await _settle(tester);

    expect(find.byKey(ShellKeys.splitter), findsNothing);
    await app.dispose(tester);
  });
}
