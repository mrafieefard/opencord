import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/ui/motion/flip_box.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';
import '../../support/floating_scenes.dart';

const _dev = 'opencord.example:7710';
const _mira = 1002, _priya = 1005, _tomas = 1006, _self = 1000;

Finder _tile(int userId, {bool screen = false}) => find.byWidgetPredicate(
  (widget) =>
      widget is VoiceTileView &&
      widget.userId == userId &&
      widget.screen == screen,
);

Finder _flip(Finder tile) =>
    find.ancestor(of: tile, matching: find.byType(FlipBox));

RenderFlipBox _box(WidgetTester tester, Finder tile) =>
    tester.renderObject<RenderFlipBox>(_flip(tile));

double _contents(WidgetTester tester, Finder tile) => tester
    .renderObject<RenderFlipBoxContents>(
      find.descendant(of: tile, matching: find.byType(FlipBoxContents)),
    )
    .opacity;

int _voiceChannel(MockApp app) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == 'General')
    .id;

Rect _rect(List<double> ltwh) =>
    Rect.fromLTWH(ltwh[0], ltwh[1], ltwh[2], ltwh[3]);

/// On the voice page in General, with every tile settled and the pointer off
/// them: the prototype's scene.
Future<MockApp> _onStage(
  WidgetTester tester, {
  Size size = const Size(1280, 800),
  AppSettings Function(AppSettings)? settings,
}) async {
  final app = await MockApp.pump(tester, size: size, settings: settings);
  await openVoicePage(SceneDriver(tester));
  return app;
}

Future<void> _settle(WidgetTester tester) async {
  for (var waited = 0; waited < 700; waited += 50) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

/// Pumps until [finder] shows, so the frame it stops on is the one it came in.
Future<void> _until(WidgetTester tester, Finder finder) async {
  for (var waited = 0; waited < 500 && finder.evaluate().isEmpty; waited++) {
    await tester.pump(const Duration(milliseconds: 1));
  }
  expect(finder, findsOneWidget);
}

Finder _control(String label) => find.descendant(
  of: find.byType(VoiceControlBar),
  matching: find.bySemanticsLabel(label),
);

/// Shares this device's screen, stopping on the frame its tile comes in.
Future<void> _goLive(WidgetTester tester) async {
  await tester.tap(_control('Share your screen'));
  await _settle(tester);
  await tester.tap(find.widgetWithText(OcButton, 'Go live'));
  await _until(tester, _tile(_self, screen: true));
}

/// Clicks a tile near its top edge: the control bar lies over the bottom of
/// the stage until the dock takes its place.
Future<void> _tapTop(WidgetTester tester, Finder tile) =>
    tester.tapAt(tester.getRect(tile).topCenter + const Offset(0, 3));

Rect _lerpMove(Rect from, Rect to, int ms) =>
    Rect.lerp(from, to, OcSpring.move.valueAt(Duration(milliseconds: ms)))!;

void main() {
  group('grid_is_16_by_10_in_at_most_1040', () {
    // measure.json's `tiles` for the prototype's 6 tiles.
    const widths = <(Size, List<double>)>[
      (Size(1440, 900), [387, 333, 760, 318]),
      (Size(1280, 800), [387, 316.3, 600, 251.3]),
      (Size(1180, 800), [387, 203.4, 500, 477.3]),
      (Size(1100, 800), [387, 297.2, 692, 289.7]),
      (Size(980, 800), [387, 169.6, 572, 544.8]),
      (Size(759, 700), [91, 256.5, 647, 270.9]),
    ];
    for (final (size, tiles) in widths) {
      testWidgets('6 tiles at ${size.width}', (tester) async {
        final app = await _onStage(tester, size: size);

        final group = tester.getRect(find.byKey(ShellKeys.tiles));
        expect(group, rectMoreOrLessEquals(_rect(tiles), epsilon: 1));
        final tile = tester.getSize(_tile(_mira));
        expect(tile.width / tile.height, closeTo(1.6, 0.001));
        expect(tester.getRect(_tile(_mira)).topLeft, group.topLeft);
        await app.dispose(tester);
      });
    }

    testWidgets('2 tiles are at most 520 wide in all', (tester) async {
      final app = await _onStage(tester, size: const Size(1440, 900));
      final prefs = find.byTooltip('Hide non-video participants');

      await tester.tap(prefs);
      await _settle(tester);

      expect(find.byType(VoiceTileView), findsNWidgets(2));
      final group = tester.getSize(find.byKey(ShellKeys.tiles));
      expect(group.width, moreOrLessEquals(520, epsilon: 0.01));
      expect(tester.getSize(_tile(_priya)), const Size(256, 160));
      await app.dispose(tester);
    });
  });

  testWidgets('tile_numbers', (tester) async {
    final app = await _onStage(tester);
    final mira = _tile(_mira);
    final colors = tester.element(mira).oc;
    final screen = _tile(_tomas, screen: true);

    BoxDecoration frame(Finder tile) =>
        tester
                .widget<Container>(
                  find
                      .descendant(of: tile, matching: find.byType(Container))
                      .first,
                )
                .decoration!
            as BoxDecoration;

    // The surface, with a 1 px border; a screen's is the chat's colour.
    expect(frame(mira).color, colors.surface);
    expect(frame(mira).borderRadius, BorderRadius.circular(14));
    expect((frame(mira).border! as Border).top.color, colors.border);
    expect((frame(mira).border! as Border).top.width, 1);
    expect(frame(screen).color, colors.chat);

    // Speaking draws a 2 px border over it.
    app.read(speakingProvider(_dev).notifier).set({_mira});
    await tester.pump();
    final speaking =
        tester
                .widget<Container>(
                  find
                      .descendant(of: mira, matching: find.byType(Container))
                      .first,
                )
                .foregroundDecoration!
            as BoxDecoration;
    expect((speaking.border! as Border).top.color, colors.text);
    expect((speaking.border! as Border).top.width, 2);

    // The name pill.
    final name = find.descendant(of: mira, matching: find.text('Mira Okafor'));
    final pill = find
        .ancestor(of: name, matching: find.byType(Container))
        .first;
    final tile = tester.getRect(mira);
    // Inside the 1 px border, as CSS's `left` and `bottom` measure.
    expect(tester.getRect(pill).left, tile.left + 1 + 8);
    expect(tester.getRect(pill).bottom, tile.bottom - 1 - 8);
    expect(tester.getRect(name).left - tester.getRect(pill).left, 9);
    expect(tester.getRect(name).top - tester.getRect(pill).top, 3);
    final pillBox = tester.widget<Container>(pill).decoration! as BoxDecoration;
    expect(pillBox.color, colors.sidebar);
    expect(pillBox.borderRadius, BorderRadius.circular(12));
    final nameStyle = tester.widget<Text>(name).style!;
    expect(nameStyle.fontSize, 12.5);
    expect(nameStyle.fontWeight, FontWeight.w600);

    // LIVE, top and right 10.
    final live = find.descendant(of: screen, matching: find.text('LIVE'));
    final liveBox = find
        .ancestor(of: live, matching: find.byType(Container))
        .first;
    expect(tester.getRect(liveBox).top, tester.getRect(screen).top + 1 + 10);
    expect(
      tester.getRect(liveBox).right,
      moreOrLessEquals(tester.getRect(screen).right - 1 - 10),
    );

    // The avatar: 72 with initials 24 even on this 121.7 tall tile.
    final initials = find.descendant(of: mira, matching: find.text('MO'));
    expect(tester.widget<Text>(initials).style!.fontSize, 24);
    expect(
      tester
          .getSize(
            find.ancestor(of: initials, matching: find.byType(Container)).first,
          )
          .width,
      72,
    );

    // Watch: 14/600, 8 above a 32 high button, padding 16, radius 10.
    final watch = find.descendant(
      of: screen,
      matching: find.widgetWithText(OcButton, 'Watch'),
    );
    final prompt = find.descendant(
      of: screen,
      matching: find.text('Tomás Ruiz is live'),
    );
    expect(tester.widget<Text>(prompt).style!.fontSize, 14);
    expect(tester.widget<Text>(prompt).style!.fontWeight, FontWeight.w600);
    expect(tester.getSize(watch).height, 32);
    expect(tester.getRect(watch).top - tester.getRect(prompt).bottom, 8);
    final label = find.descendant(of: watch, matching: find.text('Watch'));
    expect(tester.getRect(label).left - tester.getRect(watch).left, 16);
    await app.dispose(tester);
  });

  testWidgets('watch_shrinks_when_pressed', (tester) async {
    final app = await _onStage(tester);
    final watch = find.widgetWithText(OcButton, 'Watch');
    final gesture = await tester.startGesture(tester.getCenter(watch));
    // A tap counts as down once kPressTimeout has passed.
    await tester.pump(const Duration(milliseconds: 150));

    final scale = tester.widget<AnimatedScale>(
      find.descendant(of: watch, matching: find.byType(AnimatedScale)),
    );
    expect(scale.scale, 0.97);
    await gesture.cancel();
    await tester.pump(const Duration(milliseconds: 100));
    await app.dispose(tester);
  });

  testWidgets('large_text_keeps_watch_clear_of_the_name', (tester) async {
    final app = await _onStage(
      tester,
      settings: (s) => s.copyWith(fontSize: AppSettings.maxFontSize),
    );
    final screen = _tile(_tomas, screen: true);
    Rect boxOf(String text) => tester.getRect(
      find
          .ancestor(
            of: find.descendant(of: screen, matching: find.text(text)),
            matching: find.byType(Container),
          )
          .first,
    );
    final watch = tester.getRect(
      find.descendant(
        of: screen,
        matching: find.widgetWithText(OcButton, 'Watch'),
      ),
    );

    expect(watch.top, greaterThanOrEqualTo(boxOf('LIVE').bottom));
    expect(watch.bottom, lessThanOrEqualTo(boxOf('Tomás Ruiz').top));
    // No room for the words too: LIVE and the name pill say whose it is.
    expect(
      find.descendant(of: screen, matching: find.text('Tomás Ruiz is live')),
      findsNothing,
    );
    await app.dispose(tester);
  });

  group('focus_fills_above_the_strip', () {
    testWidgets('its numbers at 1280', (tester) async {
      final app = await _onStage(tester);

      await tester.tap(_tile(_priya));
      await _settle(tester);

      // measure.json's focus `tiles`: it holds both.
      final group = tester.getRect(find.byKey(ShellKeys.tiles));
      expect(
        group,
        rectMoreOrLessEquals(const Rect.fromLTWH(383, 101, 608, 682)),
      );
      expect(
        tester.getRect(_tile(_priya)),
        rectMoreOrLessEquals(
          Rect.fromLTRB(group.left, group.top, group.right, group.bottom - 84),
        ),
      );
      final strip = tester.getRect(_tile(_mira));
      expect(strip.bottom, group.bottom);
      expect(strip.height, 76);
      // Five shares the width: (608 - 4 × 8) / 5.
      expect(strip.width, moreOrLessEquals(115.2));
      expect(strip.left, moreOrLessEquals(group.left));

      // In the strip: no prompt, no LIVE, a 36 avatar, a pill at 4, 4.
      final screen = _tile(_tomas, screen: true);
      expect(
        find.descendant(of: screen, matching: find.text('LIVE')),
        findsNothing,
      );
      expect(
        find.descendant(of: screen, matching: find.text('Watch')),
        findsNothing,
      );
      final initials = find.descendant(
        of: _tile(_mira),
        matching: find.text('MO'),
      );
      expect(tester.widget<Text>(initials).style!.fontSize, 12.5);
      final name = find.descendant(
        of: _tile(_mira),
        matching: find.text('Mira Okafor'),
      );
      final pill = find
          .ancestor(of: name, matching: find.byType(Container))
          .first;
      expect(tester.getRect(pill).left, moreOrLessEquals(strip.left + 1 + 4));
      expect(tester.getRect(pill).bottom, strip.bottom - 1 - 4);
      expect(tester.widget<Text>(name).style!.fontSize, 11);
      await app.dispose(tester);
    });

    testWidgets('few share at most 200 each, centred', (tester) async {
      final app = await _onStage(tester);
      await tester.tap(find.byTooltip('Hide non-video participants'));
      await _settle(tester);
      await tester.tap(_tile(_priya));
      await _settle(tester);

      final group = tester.getRect(find.byKey(ShellKeys.tiles));
      final strip = tester.getRect(_tile(_tomas, screen: true));
      expect(strip.width, 200);
      expect(strip.center.dx, moreOrLessEquals(group.center.dx));
      await app.dispose(tester);
    });

    testWidgets('too many to share the width scroll, at 96 each', (
      tester,
    ) async {
      final app = await _onStage(tester, size: const Size(980, 800));
      await _goLive(tester);
      await _settle(tester);

      await tester.tap(_tile(_priya));
      await _settle(tester);

      // Six others in 580: 6 × 96 + 5 × 8 = 616.
      expect(tester.getSize(_tile(_mira)).width, 96);
      final strip = tester.state<ScrollableState>(
        find.descendant(
          of: find.byKey(ShellKeys.tiles),
          matching: find.byType(Scrollable),
        ),
      );
      expect(strip.position.axisDirection, AxisDirection.right);
      expect(strip.position.maxScrollExtent, moreOrLessEquals(36));
      await app.dispose(tester);
    });
  });

  testWidgets('focus_moves_on_the_spring', (tester) async {
    final app = await _onStage(tester);
    final focused = _tile(_priya);
    final other = _tile(_mira);
    final focusedFrom = tester.getRect(focused);
    final otherFrom = tester.getRect(other);

    await tester.tap(focused);
    await tester.pump();

    // The first frame is still in the old place, though layout has moved on.
    final focusedTo = tester.getRect(_flip(focused));
    final otherTo = tester.getRect(_flip(other));
    expect(focusedTo.height, greaterThan(500));
    expect(
      tester.getRect(focused),
      rectMoreOrLessEquals(focusedFrom, epsilon: 0.01),
    );
    expect(
      tester.getRect(other),
      rectMoreOrLessEquals(otherFrom, epsilon: 0.01),
    );

    await tester.pump(const Duration(milliseconds: 40));
    expect(
      tester.getRect(focused),
      rectMoreOrLessEquals(
        _lerpMove(focusedFrom, focusedTo, 40),
        epsilon: 0.01,
      ),
    );
    expect(
      tester.getRect(other),
      rectMoreOrLessEquals(_lerpMove(otherFrom, otherTo, 40), epsilon: 0.01),
    );

    await tester.pump(const Duration(milliseconds: 80));
    expect(
      tester.getRect(focused),
      rectMoreOrLessEquals(
        _lerpMove(focusedFrom, focusedTo, 120),
        epsilon: 0.01,
      ),
    );

    await tester.pump(const Duration(milliseconds: 300));
    expect(tester.getRect(focused), focusedTo);

    // And back, twice over.
    await tester.tap(focused);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    expect(
      tester.getRect(focused),
      rectMoreOrLessEquals(
        _lerpMove(focusedTo, focusedFrom, 40),
        epsilon: 0.01,
      ),
    );
    await _settle(tester);
    expect(
      tester.getRect(focused),
      rectMoreOrLessEquals(focusedFrom, epsilon: 0.01),
    );
    await tester.tap(focused);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    expect(
      tester.getRect(focused),
      rectMoreOrLessEquals(
        _lerpMove(focusedFrom, focusedTo, 40),
        epsilon: 0.01,
      ),
    );
    await _settle(tester);
    await app.dispose(tester);
  });

  testWidgets('changing course keeps the place and the speed', (tester) async {
    final app = await _onStage(tester);
    final tile = _tile(_priya);
    await tester.tap(tile);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    final before = tester.getRect(tile);
    await tester.pump(const Duration(milliseconds: 16));
    final going = tester.getRect(tile).top - before.top;
    final mid = tester.getRect(tile);

    await tester.tap(tile);
    await tester.pump();
    // The turn starts where it was.
    expect(tester.getRect(tile), rectMoreOrLessEquals(mid, epsilon: 0.01));
    await tester.pump(const Duration(milliseconds: 16));
    // Still going the way it was, a little: it hasn't jerked back.
    final next = tester.getRect(tile).top - mid.top;
    expect(next.sign, going.sign);
    await _settle(tester);
    await app.dispose(tester);
  });

  group('contents_fade_back_on_a_big_change', () {
    testWidgets('a big resize fades them back', (tester) async {
      final app = await _onStage(tester);
      final tile = _tile(_priya);
      await tester.tap(tile);
      await tester.pump();
      expect(_contents(tester, tile), 0);
      await tester.pump(const Duration(milliseconds: 70));
      expect(_contents(tester, tile), 0);
      await tester.pump(const Duration(milliseconds: 70));
      expect(
        _contents(tester, tile),
        moreOrLessEquals(Curves.easeOut.transform(0.5)),
      );
      await tester.pump(const Duration(milliseconds: 70));
      expect(_contents(tester, tile), 1);

      // The frame itself never fades.
      expect(_box(tester, tile).opacity, 1);
      await _settle(tester);
      await app.dispose(tester);
    });

    testWidgets('a move at the same size does not', (tester) async {
      final app = await _onStage(tester);
      final mira = _tile(_mira);
      final size = tester.getSize(mira);
      final top = tester.getRect(mira).top;

      // A seventh tile starts a third row: the rest go up, as big as they were.
      await _goLive(tester);

      expect(tester.getSize(mira), size);
      expect(tester.getRect(_flip(mira)).top, lessThan(top));
      expect(_contents(tester, mira), 1);
      await tester.pump(const Duration(milliseconds: 40));
      expect(tester.getRect(mira).top, lessThan(top));
      expect(_contents(tester, mira), 1);
      await _settle(tester);
      await app.dispose(tester);
    });
  });

  group('tiles_arrive_staggered', () {
    Future<void> checkStagger(WidgetTester tester) async {
      final tiles = find.byType(VoiceTileView);
      final delays = [
        for (final element in tiles.evaluate())
          (element.findAncestorWidgetOfExactType<FlipBox>()!).arriveDelay,
      ];
      expect(delays.map((delay) => delay.inMilliseconds), [
        0,
        16,
        32,
        48,
        64,
        80,
      ]);

      // At 0 ms all are there, unseen. The last waits 80 ms before it starts.
      final first = tiles.first;
      final last = tiles.last;
      expect(_box(tester, first).opacity, 0);
      expect(_box(tester, last).opacity, 0);
      await tester.pump(const Duration(milliseconds: 60));
      expect(
        _box(tester, first).opacity,
        moreOrLessEquals(Curves.easeOut.transform(0.5)),
      );
      expect(_box(tester, last).opacity, 0);
      await tester.pump(const Duration(milliseconds: 80));
      expect(
        _box(tester, last).opacity,
        moreOrLessEquals(Curves.easeOut.transform(60 / 120)),
      );
      await tester.pump(const Duration(milliseconds: 60));
      expect(_box(tester, last).opacity, 1);

      // Its place: 6 below and at 0.97 on the way, about its centre.
      final to = tester.getRect(_flip(last));
      final rise = OcSpring.tile.valueAt(const Duration(milliseconds: 120));
      final scale = 0.97 + 0.03 * rise;
      final rect = tester.getRect(last);
      expect(rect.width, moreOrLessEquals(to.width * scale, epsilon: 0.01));
      expect(
        rect.center.dy,
        moreOrLessEquals(to.center.dy + 6 * (1 - rise), epsilon: 0.01),
      );
      await _settle(tester);
      expect(tester.getRect(last), to);
    }

    testWidgets('on opening the view', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      final scene = SceneDriver(tester);
      await playText(scene);
      await scene.revealInLeft('General');
      await tester.pump();

      await scene.tap('General', scene.inLeft('General'));
      await _until(tester, _tile(_mira));
      await checkStagger(tester);
      await app.dispose(tester);
    });

    testWidgets('on joining', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      final scene = SceneDriver(tester);
      await scene.revealInLeft('General');
      await tester.pump();
      await scene.tap('General', scene.inLeft('General'));
      await _until(tester, _tile(_mira));
      await checkStagger(tester);
      await app.dispose(tester);
    });

    testWidgets('not on a window resize', (tester) async {
      final app = await _onStage(tester);
      final mira = _tile(_mira);
      final box = _box(tester, mira);

      tester.view.physicalSize = const Size(1440, 900);
      await tester.pump();

      expect(find.byType(FlipBox), findsNWidgets(6));
      expect(_box(tester, mira), same(box));
      expect(box.opacity, 1);
      expect(tester.getRect(mira), tester.getRect(_flip(mira)));
      expect(_contents(tester, mira), 1);
      await app.dispose(tester);
    });

    testWidgets('not on a pane opening', (tester) async {
      final app = await _onStage(tester);
      final mira = _tile(_mira);
      final box = _box(tester, mira);
      final scene = SceneDriver(tester);

      await scene.tap('the pane\'s close', scene.paneClose);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 40));
      expect(_box(tester, mira), same(box));
      expect(box.opacity, 1);
      expect(tester.getRect(mira), tester.getRect(_flip(mira)));
      await _settle(tester);

      await scene.tapPaneButton();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 40));
      expect(box.opacity, 1);
      expect(tester.getRect(mira), tester.getRect(_flip(mira)));
      await _settle(tester);
      await app.dispose(tester);
    });

    testWidgets('never under reduced motion', (tester) async {
      final app = await MockApp.pump(
        tester,
        size: const Size(1280, 800),
        settings: (settings) => settings.copyWith(reduceMotion: true),
      );
      final scene = SceneDriver(tester);
      await playText(scene);
      await scene.pickInLeft('General');
      await tester.tap(_tile(_priya));
      await tester.pump();
      expect(
        tester.getRect(_tile(_priya)),
        tester.getRect(_flip(_tile(_priya))),
      );
      expect(_contents(tester, _tile(_priya)), 1);
      await app.dispose(tester);
    });
  });

  group('a_new_tile_arrives_and_the_others_move', () {
    testWidgets('a camera turned on, with only cameras shown', (tester) async {
      final app = await _onStage(tester);
      await tester.tap(find.byTooltip('Hide non-video participants'));
      await _settle(tester);
      final priya = _tile(_priya);
      final from = tester.getRect(priya);
      expect(_tile(_self), findsNothing);

      await tester.tap(_control('Turn on camera'));
      final mine = _tile(_self);
      await _until(tester, mine);

      expect(_box(tester, mine).opacity, 0);
      expect(tester.getRect(priya), rectMoreOrLessEquals(from, epsilon: 0.01));
      final to = tester.getRect(_flip(priya));
      expect(to, isNot(from));
      await tester.pump(const Duration(milliseconds: 40));
      expect(
        tester.getRect(priya),
        rectMoreOrLessEquals(_lerpMove(from, to, 40), epsilon: 0.01),
      );
      await _settle(tester);
      expect(_box(tester, mine).opacity, 1);
      await app.dispose(tester);
    });

    testWidgets('a screen share started', (tester) async {
      final app = await _onStage(tester);
      final mira = _tile(_mira);
      final from = tester.getRect(mira);

      await _goLive(tester);

      final mine = _tile(_self, screen: true);
      expect(_box(tester, mine).opacity, 0);
      expect(tester.getRect(mira), rectMoreOrLessEquals(from, epsilon: 0.01));
      final to = tester.getRect(_flip(mira));
      expect(to, isNot(from));
      await _settle(tester);
      expect(tester.getRect(mira), to);
      expect(_box(tester, mine).opacity, 1);
      await app.dispose(tester);
    });
  });

  testWidgets('video_keeps_its_state', (tester) async {
    final app = await _onStage(tester);
    const feed = VideoFeed(
      trackId: 'camera-mira',
      textureId: 11,
      width: 1280,
      height: 720,
    );
    app
        .read(videoFeedsProvider.notifier)
        .apply(VideoTrackAdded(_dev, _voiceChannel(app), _mira, feed));
    await _settle(tester);
    final mira = _tile(_mira);
    final texture = find.descendant(of: mira, matching: find.byType(Texture));
    final hoverable = find.descendant(
      of: mira,
      matching: find.byType(Hoverable),
    );
    final element = tester.element(mira);
    final state = tester.state(hoverable);
    final surface = tester.renderObject(texture);

    // Into the strip, large, back to the grid and large again.
    for (final tapped in [_priya, _mira, _mira, _mira]) {
      await _tapTop(tester, _tile(tapped));
      await tester.pump();
      expect(tester.element(mira), same(element));
      expect(tester.state(hoverable), same(state));
      expect(tester.renderObject(texture), same(surface));
      await _settle(tester);
      expect(tester.element(mira), same(element));
      expect(tester.state(hoverable), same(state));
      expect(tester.renderObject(texture), same(surface));
    }

    // Focused it fits; in the grid it fills.
    FittedBox fitted() => tester.widget<FittedBox>(
      find.descendant(of: mira, matching: find.byType(FittedBox)),
    );
    expect(tester.widget<VoiceTileView>(mira).focused, isTrue);
    expect(fitted().fit, BoxFit.contain);
    await tester.tap(mira);
    await _settle(tester);
    expect(fitted().fit, BoxFit.cover);
    await app.dispose(tester);
  });

  testWidgets('stage_ignores_the_panes_animation', (tester) async {
    final app = await _onStage(tester);
    final mira = _tile(_mira);
    final scene = SceneDriver(tester);

    await scene.tap('the pane\'s close', scene.paneClose);
    await tester.pump();
    final closing = tester.getRect(mira);
    for (final ms in [40, 80, 120, 200]) {
      await tester.pump(Duration(milliseconds: ms));
      expect(tester.getRect(mira), closing);
    }
    await _settle(tester);
    expect(tester.getRect(mira), closing);

    await scene.tapPaneButton();
    await tester.pump();
    final opening = tester.getRect(mira);
    expect(opening, isNot(closing));
    for (final ms in [40, 80, 120, 200]) {
      await tester.pump(Duration(milliseconds: ms));
      expect(tester.getRect(mira), opening);
    }
    await _settle(tester);
    // Where it started, in the prototype's frame.
    expect(
      tester.getRect(find.byKey(ShellKeys.tiles)),
      rectMoreOrLessEquals(
        const Rect.fromLTWH(387, 316.3, 600, 251.3),
        epsilon: 1,
      ),
    );
    await app.dispose(tester);
  });

  testWidgets('wants_report_final_sizes', (tester) async {
    final app = await _onStage(tester);
    const feed = VideoFeed(
      trackId: 'camera-mira',
      textureId: 11,
      width: 1280,
      height: 720,
    );
    app
        .read(videoFeedsProvider.notifier)
        .apply(VideoTrackAdded(_dev, _voiceChannel(app), _mira, feed));
    await _settle(tester);
    final reports = <List<VideoWant>>[];
    final listening = app.container.listen(
      videoWantsProvider,
      (_, wants) => reports.add(wants),
    );

    await tester.tap(_tile(_mira));
    await tester.pump();
    await tester.pump();
    for (var waited = 0; waited < 500; waited += 16) {
      await tester.pump(const Duration(milliseconds: 16));
    }

    final tile = tester.getSize(_tile(_mira));
    expect(reports, hasLength(1));
    final want = reports.single.singleWhere(
      (want) => want.trackId == 'camera-mira',
    );
    expect(want.width, tile.width.round());
    expect(want.height, tile.height.round());
    listening.close();
    await app.dispose(tester);
  });

  testWidgets('a moving tile is hit where it is painted', (tester) async {
    final app = await _onStage(tester);
    final tile = _tile(_priya);
    await tester.tap(tile);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    expect(tile.hitTestable(), findsOneWidget);
    expect(
      tester.renderObject<RenderBox>(tile).localToGlobal(Offset.zero),
      tester.getRect(tile).topLeft,
    );
    await _settle(tester);
    await app.dispose(tester);
  });
}
