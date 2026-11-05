import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/notched_float.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

const _times = [0, 40, 80, 120, 160, 200, 280, 360];

// §4.3, from the running prototype.
const _openSide = [0, 59.9, 141, 197.6, 230.4, 247.6, 260.3, 264];
const _openMain = [898, 836.2, 752.7, 694.5, 660.6, 642.9, 629.8, 626];
const _openNotch = [52, 52, 39.3, 18.5, 7.3, 2.7, 0.3, 0];
const _openOpacity = [1, .86, .55, .12, 0, 0, 0, 0];
const _openScale = [1, .946, .821, .650, .6, .6, .6, .6];

const _closeSide = [264, 169.6, 75.3, 29.2, 10.5, 3.5, 0, 0];
const _closeNotch = [0, 11.8, 27.8, 38.9, 45.4, 48.8, 51.3, 52];
const _closeOpacity = [0, .57, .94, 1, 1, 1, 1, 1];
const _closeScale = [.6, .751, .904, .973, .995, 1, 1, 1];

const _musicSide = [264, 281.3, 304.6, 320.9, 330.3, 335.3, 338.9, 340];

Finder get _button => find.byTooltip('Open the right pane');
Finder get _side => find.byKey(ShellKeys.side);
Finder get _main => find.byKey(ShellKeys.main);
Finder get _notch => find.byKey(ShellKeys.notch);
Finder get _tab => find.byKey(ShellKeys.paneTab);
Finder get _panel => find.byKey(ShellKeys.panel);

Finder get _close => find.descendant(
  of: _panel,
  matching: find.byWidgetPredicate(
    (widget) => widget is OcIconButton && widget.icon == OcIcons.close,
  ),
);

Future<void> _settle(WidgetTester tester) async {
  for (var waited = 0; waited < 800; waited += 50) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

/// The frame that builds what a click changed, then the first tick: t = 0.
Future<void> _started(WidgetTester tester) async {
  await tester.pump();
  await tester.pump();
}

/// The app on #general, its pane as [open] says, at rest.
Future<MockApp> _app(
  WidgetTester tester, {
  Size size = const Size(1280, 800),
  bool open = true,
  bool reduced = false,
}) async {
  final app = await MockApp.pump(
    tester,
    size: size,
    settings: (settings) => settings.copyWith(reduceMotion: reduced),
  );
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

Future<void> _toggleWithShortcut(WidgetTester tester) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.control);
  await tester.sendKeyDownEvent(LogicalKeyboardKey.shift);
  await tester.sendKeyEvent(LogicalKeyboardKey.keyU);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.shift);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.control);
}

double _sideWidth(WidgetTester tester) => tester.getSize(_side).width;

double _mainWidth(WidgetTester tester) => tester.getSize(_main).width;

double _notchSize(WidgetTester tester) =>
    _notch.evaluate().isEmpty ? 0 : tester.getSize(_notch).width;

double _buttonOpacity(WidgetTester tester) => _tab.evaluate().isEmpty
    ? 0
    : tester
          .widget<FadeTransition>(
            find
                .ancestor(of: _tab, matching: find.byType(FadeTransition))
                .first,
          )
          .opacity
          .value;

double _buttonScale(WidgetTester tester) =>
    _tab.evaluate().isEmpty ? .6 : tester.getRect(_tab).width / 44;

void _expectRestingOpen(WidgetTester tester, {double side = 264}) {
  expect(_sideWidth(tester), side);
  expect(_mainWidth(tester), 890 - side);
  expect(_notch, findsNothing);
  expect(_tab, findsNothing);
  expect(_panel, findsOneWidget);
}

void _expectRestingShut(WidgetTester tester) {
  expect(_sideWidth(tester), 0);
  expect(_mainWidth(tester), 898);
  expect(_notchSize(tester), 52);
  expect(_tab, findsOneWidget);
  expect(_buttonScale(tester), 1);
  expect(_buttonOpacity(tester), 1);
  expect(_panel, findsNothing);
}

void main() {
  testWidgets('pane_opens_on_the_prototypes_curve', (tester) async {
    final app = await _app(tester, open: false);

    await tester.tap(_button);
    await _started(tester);
    var at = 0;
    for (final (i, t) in _times.indexed) {
      await tester.pump(Duration(milliseconds: t - at));
      at = t;
      expect(_sideWidth(tester), closeTo(_openSide[i], 2), reason: 'side $t');
      expect(_mainWidth(tester), closeTo(_openMain[i], 2), reason: 'main $t');
      expect(_notchSize(tester), closeTo(_openNotch[i], 2), reason: 'notch $t');
      expect(
        _buttonOpacity(tester),
        closeTo(_openOpacity[i], .03),
        reason: 'opacity $t',
      );
      expect(
        _buttonScale(tester),
        closeTo(_openScale[i], .03),
        reason: 'scale $t',
      );
    }
    await _settle(tester);
    _expectRestingOpen(tester);
    await app.dispose(tester);
  });

  testWidgets('the_button_goes_towards_its_top_right_corner', (tester) async {
    final app = await _app(tester, open: false);

    await tester.tap(_button);
    await _started(tester);
    await tester.pump(const Duration(milliseconds: 80));

    final main = tester.getRect(_main);
    final button = tester.getRect(_tab);
    expect(button.right, closeTo(main.right - 4, .01));
    expect(button.top, closeTo(main.top + 4, .01));
    expect(button.width, lessThan(40));
    await _settle(tester);
    await app.dispose(tester);
  });

  testWidgets('pane_closes_on_the_prototypes_curve', (tester) async {
    final app = await _app(tester);

    await tester.tap(_close);
    await _started(tester);
    var at = 0;
    for (final (i, t) in _times.indexed) {
      await tester.pump(Duration(milliseconds: t - at));
      at = t;
      expect(_sideWidth(tester), closeTo(_closeSide[i], 2), reason: 'side $t');
      expect(
        _mainWidth(tester),
        closeTo(898 - _closeSide[i] * 272 / 264, 2),
        reason: 'main $t',
      );
      expect(
        _notchSize(tester),
        closeTo(_closeNotch[i], 2),
        reason: 'notch $t',
      );
      expect(
        _buttonOpacity(tester),
        closeTo(_closeOpacity[i], .03),
        reason: 'opacity $t',
      );
      expect(
        _buttonScale(tester),
        closeTo(_closeScale[i], .03),
        reason: 'scale $t',
      );
    }
    await _settle(tester);
    _expectRestingShut(tester);
    await app.dispose(tester);
  });

  testWidgets('people_to_music_grows_on_the_curve', (tester) async {
    final app = await _app(tester);
    final server = app.read(currentServerProvider)!;
    final voice = app
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.kind == ChannelKind.voice);
    await app.read(voiceSessionProvider.notifier).join(server, voice.id);
    app.read(navigationProvider.notifier).openChannel(server, voice.id);
    await _settle(tester);
    expect(_sideWidth(tester), 264);

    app.read(rightPanelModeProvider.notifier).set(RightPanelMode.music);
    await _started(tester);
    var at = 0;
    for (final (i, t) in _times.indexed) {
      await tester.pump(Duration(milliseconds: t - at));
      at = t;
      expect(_sideWidth(tester), closeTo(_musicSide[i], 2), reason: 'side $t');
      expect(
        _mainWidth(tester),
        closeTo(890 - _musicSide[i], 2),
        reason: 'main $t',
      );
      // The float has its new width from the first frame.
      expect(tester.getSize(_panel).width, 340, reason: 'panel $t');
      if (t == 40) {
        final fade = tester.widget<FadeTransition>(
          find
              .ancestor(
                of: find.byType(MusicPanel),
                matching: find.byType(FadeTransition),
              )
              .first,
        );
        expect(
          fade.opacity.value,
          closeTo(Curves.easeOut.transform(40 / 120), .03),
        );
        expect(find.byType(MemberPanel), findsNothing);
      }
    }
    await _settle(tester);
    expect(_sideWidth(tester), 340);
    expect(_mainWidth(tester), 550);
    await app.dispose(tester);
  });

  testWidgets('quick_open_and_close_ends_exact', (tester) async {
    final app = await _app(tester, open: false);

    // Faster than any of it moves at its quickest, a frame apart.
    const frame = Duration(milliseconds: 8);
    const sideStep = 30.0;
    const notchStep = 8.0;
    var side = _sideWidth(tester);
    var main = _mainWidth(tester);
    var notch = _notchSize(tester);
    Future<void> frames(int count) async {
      for (var i = 0; i < count; i++) {
        await tester.pump(frame);
        expect((_sideWidth(tester) - side).abs(), lessThan(sideStep));
        expect((_mainWidth(tester) - main).abs(), lessThan(sideStep));
        expect((_notchSize(tester) - notch).abs(), lessThan(notchStep));
        side = _sideWidth(tester);
        main = _mainWidth(tester);
        notch = _notchSize(tester);
      }
    }

    // The X is still past the window's edge this early, so it's pressed
    // without the pointer.
    void pressClose() => tester.widget<OcIconButton>(_close).onPressed!();

    await tester.tap(_button);
    await frames(8); // 64 ms
    pressClose();
    await frames(8);
    await tester.tap(_button);
    await frames(3);
    pressClose();
    await frames(60);

    _expectRestingShut(tester);
    await app.dispose(tester);
  });

  testWidgets('the_shortcut_held_down_ends_exact', (tester) async {
    final app = await _app(tester, open: false);

    for (var i = 0; i < 9; i++) {
      await _toggleWithShortcut(tester);
      await tester.pump(const Duration(milliseconds: 33));
    }
    await _settle(tester);
    _expectRestingOpen(tester);

    await _toggleWithShortcut(tester);
    await _settle(tester);
    _expectRestingShut(tester);
    await app.dispose(tester);
  });

  testWidgets('pane_opens_closes_opens', (tester) async {
    final app = await _app(tester, open: false);

    for (var i = 0; i < 2; i++) {
      await tester.tap(_button);
      await _settle(tester);
      _expectRestingOpen(tester);
      expect(
        tester.getRect(_side),
        rectMoreOrLessEquals(const Rect.fromLTWH(1008, 40, 264, 752)),
      );
      expect(
        tester.getRect(_main),
        rectMoreOrLessEquals(const Rect.fromLTWH(374, 40, 626, 682)),
      );

      await tester.tap(_close);
      await _settle(tester);
      _expectRestingShut(tester);
      expect(
        tester.getRect(_side),
        rectMoreOrLessEquals(const Rect.fromLTWH(1272, 40, 0, 752)),
      );
      expect(
        tester.getRect(_main),
        rectMoreOrLessEquals(const Rect.fromLTWH(374, 40, 898, 682)),
      );
    }
    await app.dispose(tester);
  });

  testWidgets('notch_is_where_the_prototype_has_it', (tester) async {
    const places = [
      (Size(1280, 800), Rect.fromLTWH(1220, 40, 52, 52)),
      (Size(1100, 800), Rect.fromLTWH(1040, 40, 52, 52)),
    ];
    for (final (size, rect) in places) {
      final app = await _app(tester, size: size, open: false);

      expect(tester.getRect(_notch), rect);
      expect(tester.getRect(_tab), rect.deflate(4));

      final colors = tester.element(_tab).oc;
      final box = tester.widget<AnimatedContainer>(_tab);
      final decoration = box.decoration! as BoxDecoration;
      expect(decoration.color, colors.sidebar);
      expect(decoration.border, Border.all(color: colors.border));
      expect(decoration.boxShadow, OcShadows.float(colors));
      expect(
        decoration.borderRadius,
        const BorderRadius.only(
          topLeft: Radius.circular(10),
          topRight: Radius.circular(16),
          bottomRight: Radius.circular(10),
          bottomLeft: Radius.circular(16),
        ),
      );
      final icon = tester.widget<Icon>(
        find.descendant(of: _tab, matching: find.byType(Icon)),
      );
      expect(icon.icon, OcIcons.dockToLeft);
      expect(icon.size, 20);
      expect(icon.color, colors.textSecondary);
      await app.dispose(tester);
    }
  });

  testWidgets('the_button_lights_on_hover', (tester) async {
    final app = await _app(tester, open: false);
    final colors = tester.element(_tab).oc;

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: tester.getCenter(_tab));
    addTearDown(mouse.removePointer);
    await _settle(tester);

    final box = tester.widget<AnimatedContainer>(_tab);
    expect((box.decoration! as BoxDecoration).color, colors.selected);
    expect(
      tester
          .widget<Icon>(find.descendant(of: _tab, matching: find.byType(Icon)))
          .color,
      colors.text,
    );
    await app.dispose(tester);
  });

  testWidgets('header_buttons_clear_the_notch', (tester) async {
    final app = await _app(tester, open: false);
    final acts = find.byKey(ShellKeys.acts);
    final header = find.byKey(ShellKeys.hd);

    // `measure.json`: 1097 + 112 in text-pane-closed@wide-1280, 879 + 112 in
    // text@wide-1280. The 62 and the 8 are inside main's border.
    expect(tester.getRect(acts).right, 1209);
    expect(tester.getRect(header).right - tester.getRect(acts).right, 62);

    await tester.tap(_button);
    await _settle(tester);
    expect(tester.getRect(acts).right, 991);
    expect(tester.getRect(header).right - tester.getRect(acts).right, 8);
    await app.dispose(tester);
  });

  testWidgets('no_rewrap_while_the_pane_moves', (tester) async {
    // At 1180 the page is narrower than a bubble once the pane is open, so
    // following the frame would rewrap.
    final app = await _app(tester, size: const Size(1180, 800), open: false);
    final server = app.read(currentServerProvider)!;
    final channel = app.read(currentChannelProvider)!;
    final sent = app.repository.sendMessage(
      server,
      channel,
      'rewrap-marker ${'some words to fill the line ' * 30}',
      nonce: 'rewrap',
    );
    await _settle(tester);
    await sent;
    await _settle(tester);

    final long = find.descendant(
      of: find.byType(MessageList),
      matching: find.byWidgetPredicate(
        (widget) =>
            widget is RichText &&
            widget.text.toPlainText().contains('rewrap-marker'),
      ),
    );
    expect(long, findsOneWidget);
    int lines() {
      final paragraph = tester.renderObject<RenderParagraph>(long.first);
      final length = paragraph.text.toPlainText().length;
      return paragraph
          .getBoxesForSelection(
            TextSelection(baseOffset: 0, extentOffset: length),
          )
          .map((box) => box.top)
          .toSet()
          .length;
    }

    final shut = lines();
    await tester.tap(_button);
    await _started(tester);
    final first = lines();
    expect(first, greaterThan(shut), reason: 'it jumps once, at the start');

    var at = 0;
    for (final t in const [40, 120, 360]) {
      await tester.pump(Duration(milliseconds: t - at));
      at = t;
      expect(tester.getSize(find.byType(MessageList)).width, 524, reason: '$t');
      expect(lines(), first, reason: 'lines at $t');
    }

    // Closing, the same the other way: wider than the frame from the start.
    await tester.tap(_close);
    await _started(tester);
    final wide = lines();
    expect(wide, shut);
    at = 0;
    for (final t in const [40, 120, 360]) {
      await tester.pump(Duration(milliseconds: t - at));
      at = t;
      expect(tester.getSize(find.byType(MessageList)).width, 796, reason: '$t');
      expect(lines(), wide, reason: 'lines at $t');
    }
    await _settle(tester);
    await app.dispose(tester);
  });

  testWidgets('slot_content_keeps_its_width', (tester) async {
    final app = await _app(tester, open: false);

    await tester.tap(_button);
    await _started(tester);
    await tester.pump(const Duration(milliseconds: 40));

    expect(tester.getSize(_panel).width, 264);
    expect(tester.getTopLeft(_panel).dx, tester.getTopLeft(_side).dx);
    expect(_sideWidth(tester), lessThan(100));
    await _settle(tester);
    await app.dispose(tester);
  });

  testWidgets('the_slot_holds_its_content_only_while_open_or_moving', (
    tester,
  ) async {
    final app = await _app(tester);
    expect(_panel, findsOneWidget);

    await tester.tap(_close);
    await _started(tester);
    await tester.pump(const Duration(milliseconds: 40));
    expect(_panel, findsOneWidget);

    await _settle(tester);
    expect(_panel, findsNothing);
    await app.dispose(tester);
  });

  testWidgets('shortcut_still_toggles_people', (tester) async {
    final app = await _app(tester, open: false);

    await _toggleWithShortcut(tester);
    await _settle(tester);
    _expectRestingOpen(tester);
    expect(find.byType(MemberPanel), findsOneWidget);

    await _toggleWithShortcut(tester);
    await _settle(tester);
    _expectRestingShut(tester);
    await app.dispose(tester);
  });

  testWidgets('music_command_opens_music', (tester) async {
    final app = await _app(tester, open: false);
    final server = app.read(currentServerProvider)!;
    final channels = app.read(serverProvider(server)).data!.channels.values;
    final voice = channels.firstWhere((c) => c.kind == ChannelKind.voice);
    await app.read(voiceSessionProvider.notifier).join(server, voice.id);
    await _settle(tester);

    final input = find.descendant(
      of: find.byType(Composer),
      matching: find.byType(TextField),
    );
    await tester.enterText(input, '/music');
    await _settle(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settle(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settle(tester);

    expect(find.byType(MusicPanel), findsOneWidget);
    _expectRestingOpen(tester, side: 340);
    await app.dispose(tester);
  });

  testWidgets('no_button_in_private_messages', (tester) async {
    final app = await _app(tester, open: false);
    expect(_tab, findsOneWidget);

    app.read(navigationProvider.notifier).openHome();
    await tester.pump();
    final peer = app.repository.privateWorld.contacts.values.first.peer;
    app.read(navigationProvider.notifier).openConversation(peer);
    await _settle(tester);

    expect(_button, findsNothing);
    expect(_notch, findsNothing);
    expect(tester.widget<NotchedFloat>(_main).notch.value, 0);
    expect(_mainWidth(tester), 898);

    // Back on a server's page it is there at once: going somewhere doesn't
    // animate.
    final server = app.read(serverListProvider).first.key;
    app.read(navigationProvider.notifier).openServer(server);
    await tester.pump();
    expect(_notchSize(tester), 52);
    expect(_buttonOpacity(tester), 1);
    await _settle(tester);
    await app.dispose(tester);
  });

  testWidgets('resizing_across_1180_keeps_things_consistent', (tester) async {
    final app = await _app(tester);
    _expectRestingOpen(tester);

    tester.view.physicalSize = const Size(1100, 800);
    await _settle(tester);
    expect(
      tester.getRect(_main),
      rectMoreOrLessEquals(const Rect.fromLTWH(374, 40, 718, 682)),
    );
    expect(_side, findsNothing);
    expect(_panel, findsNothing);
    expect(_notchSize(tester), 52);
    expect(tester.getRect(_tab), const Rect.fromLTWH(1044, 44, 44, 44));

    tester.view.physicalSize = const Size(1280, 800);
    await _settle(tester);
    _expectRestingOpen(tester);
    expect(
      tester.getRect(_main),
      rectMoreOrLessEquals(const Rect.fromLTWH(374, 40, 626, 682)),
    );
    await app.dispose(tester);
  });

  testWidgets('the_drawer_shuts_the_notch_as_the_slot_does', (tester) async {
    final app = await _app(tester, size: const Size(1100, 800), open: false);
    expect(_notchSize(tester), 52);

    for (var i = 0; i < 2; i++) {
      await tester.tap(_button);
      await _settle(tester);
      expect(_notch, findsNothing);
      expect(_tab, findsNothing);
      // `measure.json`: drawer-right@medium-1100.
      expect(tester.getRect(find.byKey(ShellKeys.acts)).right, 1083);

      await tester.tap(_close);
      await _started(tester);
      await tester.pump(const Duration(milliseconds: 80));
      // Already opening while the drawer leaves.
      expect(_notchSize(tester), greaterThan(10));
      await _settle(tester);
      expect(_notchSize(tester), 52);
      expect(_buttonScale(tester), 1);
    }
    await app.dispose(tester);
  });

  testWidgets('asking_again_turns_a_leaving_drawer_back', (tester) async {
    final app = await _app(tester, size: const Size(1100, 800), open: false);
    await tester.tap(_button);
    await _settle(tester);
    final open = tester.getRect(_side);

    await tester.tap(_close);
    await _started(tester);
    await tester.pump(const Duration(milliseconds: 80));
    final leaving = tester.getRect(_side);
    expect(leaving.left, greaterThan(open.left));

    await _toggleWithShortcut(tester);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 8));
    final turned = tester.getRect(_side);
    expect((turned.left - leaving.left).abs(), lessThan(60));

    await _settle(tester);
    expect(tester.getRect(_side), open);
    expect(_notch, findsNothing);
    expect(find.byKey(ShellKeys.scrim), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('reduced_motion_jumps', (tester) async {
    final app = await _app(tester, open: false, reduced: true);

    await tester.tap(_button);
    await tester.pump();
    await tester.pump();
    _expectRestingOpen(tester);

    await tester.tap(_close);
    await tester.pump();
    await tester.pump();
    _expectRestingShut(tester);
    await app.dispose(tester);
  });
}
