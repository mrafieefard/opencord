import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/voice_view_prefs.dart';
import 'package:opencord/features/chat/chat_area.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/shell_splitter.dart';
import 'package:opencord/features/voice/voice_chat_slot.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

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

const _ms40 = Duration(milliseconds: 40);

Finder get _side => find.byKey(ShellKeys.side);
Finder get _left => find.byKey(ShellKeys.left);
Finder get _main => find.byKey(ShellKeys.main);
Finder get _sideSplitter => find.byKey(ShellKeys.sideSplitter);

double _width(WidgetTester tester, Finder finder) =>
    tester.getSize(finder).width;

/// The app on #general, its right pane as [open] says, at rest.
Future<MockApp> _onGeneral(
  WidgetTester tester, {
  Size size = const Size(1280, 800),
  KeyValueStore? store,
  bool open = true,
}) async {
  final app = await MockApp.pump(tester, size: size, store: store);
  final server = app.read(currentServerProvider)!;
  final general = app
      .read(serverProvider(server))
      .data!
      .channels
      .values
      .firstWhere((channel) => channel.name == 'general');
  app.read(navigationProvider.notifier).openChannel(server, general.id);
  if (open) {
    app.read(memberPanelProvider.notifier).open();
  } else {
    app.read(memberPanelProvider.notifier).close();
  }
  await _settle(tester);
  return app;
}

int _voiceChannel(MockApp app) => app
    .read(serverProvider(app.read(currentServerProvider)!))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.kind == ChannelKind.voice)
    .id;

/// Joins voice from #general and shows Music in the pane.
Future<void> _showMusic(WidgetTester tester, MockApp app) async {
  final server = app.read(currentServerProvider)!;
  await app
      .read(voiceSessionProvider.notifier)
      .join(server, _voiceChannel(app));
  app.read(rightPanelModeProvider.notifier).set(RightPanelMode.music);
  await _settle(tester);
}

/// The voice channel's page with its chat beside the stage.
Future<void> _showVoiceChat(WidgetTester tester, MockApp app) async {
  final server = app.read(currentServerProvider)!;
  app.read(navigationProvider.notifier).openChannel(server, _voiceChannel(app));
  await _settle(tester);
  if (!app.read(voiceChatShownProvider)) {
    app.read(voiceViewPrefsProvider.notifier).toggleChat();
  }
  await _settle(tester);
}

/// Drags the right pane's splitter by [dx] and lets go.
Future<void> _dragSide(WidgetTester tester, double dx) async {
  final drag = await tester.startGesture(tester.getCenter(_sideSplitter));
  await drag.moveBy(Offset(dx, 0));
  await tester.pump();
  await drag.up();
  await tester.pump();
}

/// What a change made in the last frame has reached [after] into its
/// spring, going from [from] to [to] on `pane`.
Future<void> _expectOnPane(
  WidgetTester tester,
  Finder finder, {
  required double from,
  required double to,
  Duration after = _ms40,
}) async {
  await tester.pump();
  await tester.pump();
  await tester.pump(after);
  expect(
    _width(tester, finder),
    closeTo(from + (to - from) * OcSpring.pane.valueAt(after), 2),
  );
}

Future<void> _focusSideSplitter(WidgetTester tester) async {
  tester
      .widget<Focus>(
        find.descendant(of: _sideSplitter, matching: find.byType(Focus)),
      )
      .focusNode!
      .requestFocus();
  await tester.pump();
}

void main() {
  group('the right pane', () {
    testWidgets('right_pane_drags_live', (tester) async {
      final app = await _onGeneral(tester);
      expect(_width(tester, _side), 264);

      final drag = await tester.startGesture(tester.getCenter(_sideSplitter));
      await drag.moveBy(const Offset(-60, 0));
      await tester.pump();
      expect(tester.getRect(_side), const Rect.fromLTWH(948, 40, 324, 752));
      expect(_width(tester, _main), 566);

      // Frame after frame, it stays where the pointer has it.
      for (var i = 0; i < 3; i++) {
        await tester.pump(const Duration(milliseconds: 16));
        expect(_width(tester, _side), 324);
      }
      await drag.moveBy(const Offset(20, 0));
      await tester.pump();
      expect(_width(tester, _side), 304);
      expect(_width(tester, _main), 586);
      // The float inside follows too, and the text rewraps live.
      expect(_width(tester, find.byKey(ShellKeys.panel)), 304);

      await drag.up();
      await _settle(tester);
      expect(_width(tester, _side), 304);
      expect(_width(tester, _main), 586);
      await app.dispose(tester);
    });

    testWidgets('its_limits_are_the_specs', (tester) async {
      var app = await _onGeneral(tester);
      final drag = await tester.startGesture(tester.getCenter(_sideSplitter));
      await drag.moveBy(const Offset(200, 0));
      await tester.pump();
      expect(_width(tester, _side), 232);
      await drag.moveBy(const Offset(-600, 0));
      await tester.pump();
      expect(_width(tester, _side), 480);
      await drag.up();
      await app.dispose(tester);

      // 1180 − 62 − 4·8 − 288 − 380.
      app = await _onGeneral(tester, size: const Size(1180, 800));
      expect(_width(tester, _left), 288);
      await _dragSide(tester, -400);
      expect(_width(tester, _side), 418);
      expect(_width(tester, _left), 288);
      await app.dispose(tester);

      app = await _onGeneral(tester);
      await _showMusic(tester, app);
      expect(_width(tester, _side), 340);
      await _dragSide(tester, 200);
      expect(_width(tester, _side), 300);
      await app.dispose(tester);

      // 1440 − 62 − 4·8 − 288 − 600, where 380 would give 480.
      app = await _onGeneral(tester, size: const Size(1440, 900));
      await _showVoiceChat(tester, app);
      expect(find.byType(VoiceChatSlot), findsOneWidget);
      expect(_width(tester, _side), 380);
      await _dragSide(tester, -400);
      expect(_width(tester, _side), 458);
      await _dragSide(tester, 400);
      expect(_width(tester, _side), 300);
      await app.dispose(tester);
    });

    testWidgets('each_content_keeps_its_own_width', (tester) async {
      final app = await _onGeneral(tester);
      await _dragSide(tester, -36);
      await _settle(tester);
      expect(_width(tester, _side), 300);

      await app
          .read(voiceSessionProvider.notifier)
          .join(app.read(currentServerProvider)!, _voiceChannel(app));
      await _settle(tester);
      app.read(rightPanelModeProvider.notifier).set(RightPanelMode.music);
      await _expectOnPane(tester, _side, from: 300, to: 340);
      await _settle(tester);
      expect(_width(tester, _side), 340);

      await _dragSide(tester, -60);
      await _settle(tester);
      expect(_width(tester, _side), 400);

      app.read(rightPanelModeProvider.notifier).set(RightPanelMode.people);
      await _expectOnPane(tester, _side, from: 400, to: 300);
      await _settle(tester);
      expect(_width(tester, _side), 300);

      app.read(rightPanelModeProvider.notifier).set(RightPanelMode.music);
      await _settle(tester);
      expect(_width(tester, _side), 400);
      await app.dispose(tester);
    });

    testWidgets('its_widths_are_remembered', (tester) async {
      final store = CountingStore();
      var app = await _onGeneral(tester, store: store);
      var writes = store.writeCount;

      // A click without a drag writes nothing.
      final click = await tester.startGesture(tester.getCenter(_sideSplitter));
      await click.up();
      await tester.pump(const Duration(milliseconds: 500));
      expect(store.writeCount, writes);

      final drag = await tester.startGesture(tester.getCenter(_sideSplitter));
      for (var i = 0; i < 3; i++) {
        await drag.moveBy(const Offset(-10, 0));
        await tester.pump();
      }
      expect(store.writeCount, writes);
      await drag.up();
      await tester.pump();
      expect(store.writeCount, writes + 1);
      expect(store.read('ui.sideWidth.people'), '294.0');

      await _showMusic(tester, app);
      writes = store.writeCount;
      await _dragSide(tester, -20);
      expect(store.writeCount, writes + 1);
      expect(store.read('ui.sideWidth.music'), '360.0');
      await app.dispose(tester);

      app = await _onGeneral(tester, size: const Size(1440, 900), store: store);
      await _showVoiceChat(tester, app);
      writes = store.writeCount;
      await _dragSide(tester, -20);
      expect(store.writeCount, writes + 1);
      expect(store.read('ui.sideWidth.voiceChat'), '400.0');
      expect(store.read('ui.sideWidth.people'), '294.0');
      await app.dispose(tester);

      // The next launch opens them where they were left.
      app = await _onGeneral(tester, store: store);
      expect(_width(tester, _side), 294);
      await _showMusic(tester, app);
      expect(_width(tester, _side), 360);
      await app.dispose(tester);
    });

    testWidgets('double_click_and_home_reset_on_the_spring', (tester) async {
      final store = MemoryKeyValueStore();
      store.write('ui.sideWidth.people', '350.0');
      final app = await _onGeneral(tester, store: store);
      expect(_width(tester, _side), 350);

      await tester.tap(_sideSplitter);
      await tester.pump(const Duration(milliseconds: 50));
      await tester.tap(_sideSplitter);
      await _expectOnPane(tester, _side, from: 350, to: 264);
      await _settle(tester);
      expect(_width(tester, _side), 264);
      expect(store.read('ui.sideWidth.people'), isNull);

      await _dragSide(tester, -86);
      await _settle(tester);
      expect(_width(tester, _side), 350);
      await _focusSideSplitter(tester);
      await tester.sendKeyEvent(LogicalKeyboardKey.home);
      await _expectOnPane(tester, _side, from: 350, to: 264);
      await _settle(tester);
      expect(_width(tester, _side), 264);
      expect(store.read('ui.sideWidth.people'), isNull);
      await app.dispose(tester);
    });

    testWidgets('arrow_left_grows_it', (tester) async {
      final store = MemoryKeyValueStore();
      final app = await _onGeneral(tester, store: store);
      await _focusSideSplitter(tester);

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
      await _settle(tester);
      expect(_width(tester, _side), 280);
      expect(store.read('ui.sideWidth.people'), '280.0');

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
      await _settle(tester);
      expect(_width(tester, _side), 296);

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await _settle(tester);
      expect(_width(tester, _side), 280);
      expect(store.read('ui.sideWidth.people'), '280.0');
      // The left column's keys are its own.
      expect(_width(tester, _left), 288);
      await app.dispose(tester);
    });

    testWidgets('it_is_a_slider_for_screen_readers', (tester) async {
      final app = await _onGeneral(tester);
      expect(
        tester.getSemantics(find.bySemanticsLabel('Resize the right pane')),
        isSemantics(
          label: 'Resize the right pane',
          isSlider: true,
          value: '264',
          increasedValue: '280',
          decreasedValue: '248',
          hasIncreaseAction: true,
          hasDecreaseAction: true,
        ),
      );
      expect(
        tester.getSemantics(find.bySemanticsLabel('Resize the channel list')),
        isSemantics(
          label: 'Resize the channel list',
          isSlider: true,
          value: '288',
        ),
      );
      await app.dispose(tester);
    });

    testWidgets('no_splitter_on_a_drawer', (tester) async {
      const expected = [
        (Size(1100, 800), Rect.fromLTWH(828, 40, 264, 752)),
        (Size(980, 800), Rect.fromLTWH(708, 40, 264, 752)),
        (Size(940, 560), Rect.fromLTWH(668, 40, 264, 512)),
        (Size(759, 700), Rect.fromLTWH(487, 40, 264, 652)),
      ];
      for (final (size, rect) in expected) {
        final app = await _onGeneral(tester, size: size, open: false);
        await tester.tap(find.byTooltip('Open the right pane'));
        await _settle(tester);
        expect(
          tester.getRect(_side),
          rectMoreOrLessEquals(rect, epsilon: 1),
          reason: '$size',
        );
        expect(_sideSplitter, findsNothing, reason: '$size');
        await app.dispose(tester);
      }
    });

    testWidgets('no_splitter_while_shut', (tester) async {
      final app = await _onGeneral(tester, open: false);
      expect(_sideSplitter, findsNothing);

      for (var i = 0; i < 2; i++) {
        await tester.tap(find.byTooltip('Open the right pane'));
        await tester.pump();
        expect(_sideSplitter, findsOneWidget);
        // At the pane's resting edge, from the first frame.
        expect(tester.getRect(_sideSplitter).right, 1272 - 264);
        await _settle(tester);
        expect(_sideSplitter, findsOneWidget);

        app.read(memberPanelProvider.notifier).close();
        await tester.pump();
        expect(_sideSplitter, findsNothing);
        await _settle(tester);
        expect(_sideSplitter, findsNothing);
      }
      await app.dispose(tester);
    });

    testWidgets('dragging_then_closing_ends_exact', (tester) async {
      final app = await _onGeneral(tester);
      final notch = find.byKey(ShellKeys.notch);
      Finder close() => find.descendant(
        of: find.byKey(ShellKeys.panel),
        matching: find.byWidgetPredicate(
          (widget) => widget is OcIconButton && widget.icon == OcIcons.close,
        ),
      );

      for (var i = 0; i < 2; i++) {
        final drag = await tester.startGesture(tester.getCenter(_sideSplitter));
        await drag.moveBy(Offset(i == 0 ? -60 : 10, 0));
        await tester.pump();
        await drag.up();
        // Closed the moment it's let go, opened again mid-spring.
        tester.widget<OcIconButton>(close()).onPressed!();
        for (var frame = 0; frame < 6; frame++) {
          await tester.pump(const Duration(milliseconds: 16));
        }
        await tester.tap(find.byTooltip('Open the right pane'));
        await _settle(tester);
        final open = i == 0 ? 324.0 : 314.0;
        expect(_width(tester, _side), open);
        expect(_width(tester, _main), 890 - open);
        expect(notch, findsNothing);

        tester.widget<OcIconButton>(close()).onPressed!();
        await _settle(tester);
        expect(_width(tester, _side), 0);
        expect(_width(tester, _main), 898);
        expect(tester.getSize(notch).width, 52);

        await tester.tap(find.byTooltip('Open the right pane'));
        await _settle(tester);
        expect(_width(tester, _side), open);
        expect(notch, findsNothing);
      }
      await app.dispose(tester);
    });

    testWidgets('window_shrinking_gives_way_and_returns', (tester) async {
      final store = MemoryKeyValueStore();
      store.write('ui.sideWidth.people', '480.0');
      var app = await _onGeneral(tester, store: store);
      expect(_width(tester, _side), 480);
      expect(_width(tester, _left), 288);

      tester.view.physicalSize = const Size(1180, 800);
      await _expectOnPane(tester, _side, from: 480, to: 418);
      expect(_width(tester, _left), 288);
      await _settle(tester);
      expect(_width(tester, _side), 418);
      expect(_width(tester, _left), 288);

      tester.view.physicalSize = const Size(1280, 800);
      await _expectOnPane(tester, _side, from: 418, to: 480);
      await _settle(tester);
      expect(_width(tester, _side), 480);
      expect(store.read('ui.sideWidth.people'), '480.0');
      await app.dispose(tester);

      // Past the pane's minimum, the left column gives way.
      store.write(sidebarWidthKey, '420.0');
      app = await _onGeneral(tester, store: store);
      await _showMusic(tester, app);
      expect(_width(tester, _side), 340);
      expect(_width(tester, _left), 420);

      tester.view.physicalSize = const Size(1180, 800);
      await _expectOnPane(tester, _side, from: 340, to: 300);
      expect(
        _width(tester, _left),
        closeTo(420 - 14 * OcSpring.pane.valueAt(_ms40), 2),
      );
      await _settle(tester);
      expect(_width(tester, _side), 300);
      expect(_width(tester, _left), 406);

      tester.view.physicalSize = const Size(1280, 800);
      await _settle(tester);
      expect(_width(tester, _side), 340);
      expect(_width(tester, _left), 420);
      await app.dispose(tester);
    });

    testWidgets('dragging_a_voice_chat_never_flips_it', (tester) async {
      final app = await _onGeneral(tester, size: const Size(1440, 900));
      await _showVoiceChat(tester, app);
      expect(find.byType(VoiceChatSlot), findsOneWidget);

      final drag = await tester.startGesture(tester.getCenter(_sideSplitter));
      await drag.moveBy(const Offset(-400, 0));
      await tester.pump();
      expect(_width(tester, _side), 458);
      expect(find.byType(VoiceChatSlot), findsOneWidget);
      await drag.up();
      await _settle(tester);
      expect(_width(tester, _side), 458);
      expect(find.byType(VoiceChatSlot), findsOneWidget);
      expect(
        find.descendant(of: _side, matching: find.byType(VoiceChatSlot)),
        findsOneWidget,
      );
      await app.dispose(tester);
    });

    testWidgets('the_left_limit_follows_the_pane', (tester) async {
      final app = await _onGeneral(tester, size: const Size(1180, 800));
      await _dragSide(tester, -400);
      await _settle(tester);
      expect(_width(tester, _side), 418);

      // 1180 − 62 − 4·8 − 418 − 380: no room to grow.
      var drag = await tester.startGesture(
        tester.getCenter(find.byKey(ShellKeys.splitter)),
      );
      await drag.moveBy(const Offset(200, 0));
      await tester.pump();
      expect(_width(tester, _left), 288);
      await drag.up();
      await _settle(tester);

      // With the pane at its minimum, up to the column's own 420.
      await _dragSide(tester, 400);
      await _settle(tester);
      expect(_width(tester, _side), 232);
      drag = await tester.startGesture(
        tester.getCenter(find.byKey(ShellKeys.splitter)),
      );
      await drag.moveBy(const Offset(100, 0));
      await tester.pump();
      expect(_width(tester, _left), 388);
      await drag.moveBy(const Offset(200, 0));
      await tester.pump();
      expect(_width(tester, _left), 420);
      await drag.up();
      await _settle(tester);
      expect(_width(tester, _left), 420);
      expect(_width(tester, _side), 232);
      await app.dispose(tester);
    });
  });

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

  testWidgets('the_hit_area_is_the_gap_and_4_each_side', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await _settle(tester);
    final gap = tester.getRect(find.byKey(ShellKeys.splitter));
    expect(gap.width, 8.0);
    final y = gap.center.dy;

    for (final (x, grows) in [
      (gap.left - 3, true),
      (gap.right + 3, true),
      (gap.left - 5, false),
      (gap.right + 5, false),
    ]) {
      final before = tester.getRect(find.byKey(ShellKeys.left)).width;
      final drag = await tester.startGesture(Offset(x, y));
      await drag.moveBy(const Offset(10, 0));
      await tester.pump();
      final after = tester.getRect(find.byKey(ShellKeys.left)).width;
      expect(after - before, grows ? 10.0 : 0.0, reason: 'at $x');
      await drag.moveBy(const Offset(-10, 0));
      await drag.up();
      await _settle(tester);
    }

    await app.dispose(tester);
  });

  testWidgets('past_the_limit_the_column_waits_for_the_pointer', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await _settle(tester);

    final drag = await tester.startGesture(
      tester.getCenter(find.byKey(ShellKeys.splitter)),
    );
    await drag.moveBy(const Offset(-200, 0));
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 232.0);

    // Still 144 px left of where the limit was reached.
    await drag.moveBy(const Offset(10, 0));
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 232.0);

    await drag.moveBy(const Offset(170, 0));
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 268.0);
    await drag.up();

    await app.dispose(tester);
  });

  testWidgets('a_click_that_moves_nothing_writes_nothing', (tester) async {
    final store = CountingStore();
    final app = await MockApp.pump(
      tester,
      size: const Size(1280, 800),
      store: store,
    );
    await _settle(tester);
    final writes = store.writeCount;

    final splitter = tester.getCenter(find.byKey(ShellKeys.splitter));
    final click = await tester.startGesture(splitter);
    await click.up();
    await tester.pump(const Duration(milliseconds: 500));
    expect(store.writeCount, writes);
    expect(store.read(sidebarWidthKey), isNull);

    await app.dispose(tester);
  });

  testWidgets('only_the_main_button_drags', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await _settle(tester);

    final drag = await tester.startGesture(
      tester.getCenter(find.byKey(ShellKeys.splitter)),
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await drag.moveBy(const Offset(30, 0));
    await tester.pump();
    expect(tester.getRect(find.byKey(ShellKeys.left)).width, 288.0);
    await drag.up();

    await app.dispose(tester);
  });

  testWidgets('reduced_motion_shows_the_pill_at_once', (tester) async {
    tester.platformDispatcher.accessibilityFeaturesTestValue =
        const FakeAccessibilityFeatures(disableAnimations: true);
    addTearDown(tester.platformDispatcher.clearAccessibilityFeaturesTestValue);
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await _settle(tester);

    final splitter = find.byKey(ShellKeys.splitter);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(splitter));
    await tester.pump();
    await tester.pump();
    expect(
      tester
          .renderObject<RenderAnimatedOpacity>(
            find.descendant(
              of: splitter,
              matching: find.byType(AnimatedOpacity),
            ),
          )
          .opacity
          .value,
      1.0,
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
