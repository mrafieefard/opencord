import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/soundboard/soundboard_picker.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';
import '../../support/floating_scenes.dart';

const _dev = 'opencord.example:7710';

Finder _dock() => find.byKey(ShellKeys.dock);

Finder _control(String label) => find.descendant(
      of: _dock(),
      matching: find.bySemanticsLabel(label),
    );

Future<MockApp> _onVoice(
  WidgetTester tester, {
  Size size = const Size(1280, 800),
}) async {
  final app = await MockApp.pump(tester, size: size);
  await openVoicePage(SceneDriver(tester));
  return app;
}

Future<void> _pumpTime(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

bool _dockVisible(WidgetTester tester) {
  final ignore = tester.widget<IgnorePointer>(
    find.ancestor(of: _dock(), matching: find.byType(IgnorePointer)).first,
  );
  return !ignore.ignoring;
}

double _dockOpacity(WidgetTester tester) {
  final opacity = tester.widget<Opacity>(
    find.ancestor(of: _dock(), matching: find.byType(Opacity)).first,
  );
  return opacity.opacity;
}

double _dockTranslateY(WidgetTester tester) {
  final transform = tester.widget<Transform>(
    find.ancestor(of: _dock(), matching: find.byType(Transform)).first,
  );
  return transform.transform.getTranslation().y;
}

void main() {
  testWidgets('dock_numbers', (tester) async {
    final app = await _onVoice(tester);
    expect(_dock(), findsOneWidget);

    final dockBox = tester.getRect(_dock());
    final mainBox = tester.getRect(find.byKey(ShellKeys.main));

    // Height 64.
    expect(dockBox.height, 64.0);

    // Centred in the main float within 1 px.
    expect(dockBox.center.dx, moreOrLessEquals(mainBox.center.dx, epsilon: 1.0));

    // Bottom edge 15 above the float's bottom edge (14 and its 1 px border).
    expect(mainBox.bottom - dockBox.bottom, moreOrLessEquals(15.0, epsilon: 1.0));

    // With the chat button its width is 517.7 within 2.
    expect((dockBox.width - 517.7).abs(), lessThanOrEqualTo(2.0));

    // Round buttons are 48x48.
    final muteBtn = tester.getRect(_control('Mute'));
    expect(muteBtn.width, 48.0);
    expect(muteBtn.height, 48.0);

    // Separator line: 1 px wide, 28 px tall.
    final divider = find.descendant(
      of: _dock(),
      matching: find.byWidgetPredicate(
        (w) => w is Container && w.constraints?.maxWidth == 1 && w.constraints?.maxHeight == 28,
      ),
    );
    expect(divider, findsOneWidget);

    // Disconnect button.
    final disconnect = find.descendant(
      of: _dock(),
      matching: find.text('Disconnect'),
    );
    expect(disconnect, findsOneWidget);
    final disconnectBox = tester.getRect(
      find.ancestor(of: disconnect, matching: find.byType(Hoverable)).first,
    );
    expect(disconnectBox.height, 48.0);

    // On states: Mute lights up when muted.
    await tester.tap(_control('Mute'));
    await tester.pumpAndSettle();
    expect(app.read(voiceSessionProvider).muted, isTrue);
    expect(_control('Unmute'), findsOneWidget);

    // Deafen lights up when deafened.
    await tester.tap(_control('Deafen'));
    await tester.pumpAndSettle();
    expect(app.read(voiceSessionProvider).deafened, isTrue);
    expect(_control('Undeafen'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('moving_shows_and_resting_hides', (tester) async {
    final app = await _onVoice(tester);
    final stageCenter = tester.getCenter(find.byKey(ShellKeys.stage));

    // Initially visible after opening the page.
    expect(_dockVisible(tester), isTrue);

    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(gesture.removePointer);
    await gesture.addPointer(location: stageCenter);
    await gesture.moveTo(stageCenter + const Offset(1, 1));
    await tester.pump();

    // Resting for 2.0 s: still visible.
    await _pumpTime(tester, const Duration(milliseconds: 2000));
    expect(_dockVisible(tester), isTrue);

    // Moving at 2.0 s restarts the timer.
    await gesture.moveTo(stageCenter + const Offset(10, 10));
    await tester.pump();

    // Another 2.0 s: still visible.
    await _pumpTime(tester, const Duration(milliseconds: 2000));
    expect(_dockVisible(tester), isTrue);

    // Another 0.6 s (2.6 s total without moves): now hidden.
    await _pumpTime(tester, const Duration(milliseconds: 600));
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);

    // Twice over: moving again shows it.
    await gesture.moveTo(stageCenter + const Offset(20, 20));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 150));
    expect(_dockVisible(tester), isTrue);

    // Resting for 2.0 s: still visible.
    await _pumpTime(tester, const Duration(milliseconds: 2000));
    expect(_dockVisible(tester), isTrue);

    // Resting 0.6 s more: hides.
    await _pumpTime(tester, const Duration(milliseconds: 600));
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);

    await app.dispose(tester);
  });

  testWidgets('opening_the_page_shows_it_first', (tester) async {
    final app = await _onVoice(tester);

    // Opening the page counts as a move, so it shows first.
    expect(_dockVisible(tester), isTrue);
    expect(_dockOpacity(tester), 1.0);
    expect(_dockTranslateY(tester), 0.0);

    // Without movement, after 2.5 s it hides.
    await _pumpTime(tester, const Duration(milliseconds: 2600));
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);

    await app.dispose(tester);
  });

  testWidgets('leaving_the_float_hides', (tester) async {
    final app = await _onVoice(tester);
    final stageCenter = tester.getCenter(find.byKey(ShellKeys.stage));

    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(gesture.removePointer);
    await gesture.addPointer(location: stageCenter);
    await gesture.moveTo(stageCenter + const Offset(1, 1));
    await tester.pump();
    expect(_dockVisible(tester), isTrue);

    // Moving over the header: stays, because the header is in the float.
    final headerCenter = tester.getCenter(find.byKey(ShellKeys.hd));
    await gesture.moveTo(headerCenter);
    await tester.pump();
    expect(_dockVisible(tester), isTrue);

    // Moving outside the main float (to rail at x=20, y=100) hides immediately.
    await gesture.moveTo(const Offset(20, 100));
    await tester.pump();
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);

    await app.dispose(tester);
  });

  testWidgets('pointer_on_it_keeps_it', (tester) async {
    final app = await _onVoice(tester);
    final dockCenter = tester.getCenter(_dock());

    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(gesture.removePointer);
    await gesture.addPointer(location: dockCenter);
    await tester.pump();
    expect(_dockVisible(tester), isTrue);

    // 5 seconds pass with pointer on it: NEVER hides.
    await _pumpTime(tester, const Duration(seconds: 5));
    expect(_dockVisible(tester), isTrue);

    // Move pointer off dock to stage center.
    final stageCenter = tester.getCenter(find.byKey(ShellKeys.stage));
    await gesture.moveTo(stageCenter);
    await tester.pump();

    // 2.0 s: still visible.
    await _pumpTime(tester, const Duration(milliseconds: 2000));
    expect(_dockVisible(tester), isTrue);

    // 0.6 s more: hides.
    await _pumpTime(tester, const Duration(milliseconds: 600));
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);

    await app.dispose(tester);
  });

  testWidgets('focus_in_it_keeps_it', (tester) async {
    final app = await _onVoice(tester);
    expect(_dockVisible(tester), isTrue);

    // Focus the Mute button.
    final muteHoverable = find.descendant(
      of: _control('Mute'),
      matching: find.byType(Hoverable),
    );
    final focusNode = tester.widget<Hoverable>(muteHoverable).focusNode ??
        Focus.of(tester.element(muteHoverable));
    focusNode.requestFocus();
    await tester.pump();

    // Pointer is off the float (Offset(10, 10)).
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(gesture.removePointer);
    await gesture.addPointer(location: const Offset(10, 10));
    await tester.pump();

    // 5 seconds pass: focus inside dock keeps it visible.
    await _pumpTime(tester, const Duration(seconds: 5));
    expect(_dockVisible(tester), isTrue);

    // Unfocus.
    focusNode.unfocus();
    await tester.pump();

    // After 2.5 s of losing focus without pointer on float: hides.
    await _pumpTime(tester, const Duration(milliseconds: 2600));
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);

    await app.dispose(tester);
  });

  testWidgets('dock_stays_while_its_popup_is_open', (tester) async {
    final app = await _onVoice(tester);
    expect(_dockVisible(tester), isTrue);

    // Open soundboard picker.
    await tester.tap(_control('Soundboard'));
    await tester.pumpAndSettle();
    expect(find.byType(SoundboardPicker), findsOneWidget);

    // Advance 5 seconds: dock stays while popup is open.
    await _pumpTime(tester, const Duration(seconds: 5));
    expect(_dockVisible(tester), isTrue);

    // Close popup by tapping barrier.
    await tester.tapAt(const Offset(50, 50));
    await tester.pumpAndSettle();
    expect(find.byType(SoundboardPicker), findsNothing);

    // 2.0 s after closing: still visible.
    await _pumpTime(tester, const Duration(milliseconds: 2000));
    expect(_dockVisible(tester), isTrue);

    // 0.6 s more (2.6 s total after close): hides.
    await _pumpTime(tester, const Duration(milliseconds: 600));
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);

    await app.dispose(tester);
  });

  testWidgets('hidden_it_takes_no_clicks', (tester) async {
    final app = await _onVoice(tester);

    // Wait for dock to hide.
    await _pumpTime(tester, const Duration(milliseconds: 2600));
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);

    // Tapping at dock center passes through.
    final dockCenter = tester.getCenter(_dock());
    await tester.tapAt(dockCenter);
    await tester.pump();

    // Dock was not clicked (mute state didn't change).
    expect(app.read(voiceSessionProvider).muted, isFalse);

    await app.dispose(tester);
  });

  testWidgets('shows_and_hides_on_the_curves', (tester) async {
    final app = await _onVoice(tester);
    final stageCenter = tester.getCenter(find.byKey(ShellKeys.stage));

    // Wait for dock to hide completely.
    await _pumpTime(tester, const Duration(milliseconds: 2600));
    await tester.pumpAndSettle();
    expect(_dockVisible(tester), isFalse);
    expect(_dockTranslateY(tester), 16.0);
    expect(_dockOpacity(tester), 0.0);

    // Move pointer over float to show it.
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(gesture.removePointer);
    await gesture.addPointer(location: stageCenter);
    await gesture.moveTo(stageCenter + const Offset(1, 1));
    await tester.pump();

    // Showing: pop spring rises translateY(16 -> 0), opacity 100 ms easeOut.
    await tester.pump(const Duration(milliseconds: 50));
    expect(_dockTranslateY(tester), lessThan(16.0));
    expect(_dockOpacity(tester), greaterThan(0.0));

    await tester.pumpAndSettle();
    expect(_dockTranslateY(tester), moreOrLessEquals(0.0, epsilon: 0.1));
    expect(_dockOpacity(tester), 1.0);

    // Move outside float to hide.
    await gesture.moveTo(const Offset(10, 10));
    await tester.pump();

    // Hiding: 130 ms easeIn for both translateY(0 -> 16) and opacity(1 -> 0).
    await tester.pump(const Duration(milliseconds: 65));
    expect(_dockTranslateY(tester), greaterThan(0.0));
    expect(_dockTranslateY(tester), lessThan(16.0));
    expect(_dockOpacity(tester), lessThan(1.0));
    expect(_dockOpacity(tester), greaterThan(0.0));

    await tester.pumpAndSettle();
    expect(_dockTranslateY(tester), 16.0);
    expect(_dockOpacity(tester), 0.0);

    await app.dispose(tester);
  });

  testWidgets('press_scales_it', (tester) async {
    final app = await _onVoice(tester);
    final muteCenter = tester.getCenter(_control('Mute'));

    final gesture = await tester.startGesture(muteCenter);
    await tester.pump();
    await tester.pump(kPressTimeout);
    await tester.pump(const Duration(milliseconds: 80));

    // Press scale is 0.97 over 80 ms.
    final scale = tester.widget<AnimatedScale>(
      find.descendant(
        of: _control('Mute'),
        matching: find.byType(AnimatedScale),
      ),
    );
    expect(scale.scale, 0.97);

    await gesture.up();
    await tester.pumpAndSettle();

    // Tapping mute toggled it to 'Unmute'.
    final scaleAfter = tester.widget<AnimatedScale>(
      find.descendant(
        of: _control('Unmute'),
        matching: find.byType(AnimatedScale),
      ),
    );
    expect(scaleAfter.scale, 1.0);

    await app.dispose(tester);
  });

  testWidgets('not_joined_has_no_dock, dock_leaves_no_timer_behind', (tester) async {
    final app = await MockApp.pump(tester);
    // Before joining General, open channel in view without joining.
    final dev = app.read(serverProvider(_dev)).data!;
    final general = dev.channels.values
        .firstWhere((c) => c.name == 'General')
        .id;
    app.read(navigationProvider.notifier).openChannel(_dev, general);
    await tester.pumpAndSettle();
    expect(_dock(), findsNothing);

    // Join General so dock appears and timer is running.
    await tester.tap(find.widgetWithText(OcButton, 'Join voice'));
    await tester.pump();
    expect(_dock(), findsOneWidget);

    // Dispose while timer is actively running: no leaks or exceptions.
    await app.dispose(tester);
  });

  testWidgets('narrow_page_scrolls_the_dock', (tester) async {
    // Narrow window where main float is narrower than the full 518 px dock.
    final app = await _onVoice(tester, size: const Size(600, 700));
    expect(_dock(), findsOneWidget);

    final dockBox = tester.getRect(_dock());
    final mainBox = tester.getRect(find.byKey(ShellKeys.main));

    // Dock is at most page width - 24.
    expect(dockBox.width, lessThanOrEqualTo(mainBox.width - 24 + 1.0));

    // SingleChildScrollView allows horizontal scrolling.
    final scrollable = find.descendant(
      of: _dock(),
      matching: find.byType(SingleChildScrollView),
    );
    expect(scrollable, findsOneWidget);

    await app.dispose(tester);
  });
}
