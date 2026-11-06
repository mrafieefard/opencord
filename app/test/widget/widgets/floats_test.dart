import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/notched_float.dart';
import 'package:opencord/ui/widgets/oc_float.dart';

import '../../support/pump.dart';

/// The prototype's shadows, as the floating UI's design gives them (§2.3).
const _floatShadows = {
  'dark': [
    BoxShadow(color: Color(0x73000000), offset: Offset(0, 8), blurRadius: 19.9),
  ],
  'light': [
    BoxShadow(color: Color(0x14111114), offset: Offset(0, 6), blurRadius: 16.4),
  ],
};

const _menuShadows = {
  'dark': [
    BoxShadow(color: Color(0x80000000), offset: Offset(0, 4), blurRadius: 13),
  ],
  'light': [
    BoxShadow(color: Color(0x1F111114), offset: Offset(0, 4), blurRadius: 13),
  ],
};

const _box = Size(600, 400);

Path _notched(double notch, {double inset = 0}) =>
    notchPath(_box, radius: 16, notch: notch, inset: inset);

/// The float under test, placed at [_origin] in a window of [_window].
const _origin = Offset(20, 30);
const _window = Size(640, 460);

Widget _placed(Widget float) => Stack(
  children: [
    Positioned(
      left: _origin.dx,
      top: _origin.dy,
      width: _box.width,
      height: _box.height,
      child: float,
    ),
  ],
);

/// The straight RGBA bytes drawn under [boundary].
Future<(ByteData, int)> _pixels(WidgetTester tester, GlobalKey boundary) async {
  final render =
      boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  final image = (await tester.runAsync(() => render.toImage()))!;
  final bytes = (await tester.runAsync(
    () => image.toByteData(format: ui.ImageByteFormat.rawStraightRgba),
  ))!;
  final width = image.width;
  image.dispose();
  return (bytes, width);
}

Color _pixel((ByteData, int) pixels, Offset at) {
  final (bytes, width) = pixels;
  final i = (at.dy.floor() * width + at.dx.floor()) * 4;
  return Color.fromARGB(
    bytes.getUint8(i + 3),
    bytes.getUint8(i),
    bytes.getUint8(i + 1),
    bytes.getUint8(i + 2),
  );
}

/// Whether two colours are the same to within [within] per channel (0–255).
bool _near(Color a, Color b, {int within = 3}) {
  int c(double v) => (v * 255).round();
  return (c(a.r) - c(b.r)).abs() <= within &&
      (c(a.g) - c(b.g)).abs() <= within &&
      (c(a.b) - c(b.b)).abs() <= within;
}

void main() {
  testWidgets('float_has_the_spec_surface', (tester) async {
    for (final (name, colors) in themes) {
      // A fresh app, so the theme doesn't animate from the last one.
      await tester.pumpWidget(const SizedBox());
      await pumpThemed(
        tester,
        colors: colors,
        const Center(
          child: SizedBox(
            width: 300,
            height: 200,
            child: OcFloat(child: SizedBox.expand()),
          ),
        ),
      );

      final box = tester.widget<DecoratedBox>(
        find.descendant(
          of: find.byType(OcFloat),
          matching: find.byType(DecoratedBox),
        ),
      );
      final decoration = box.decoration as BoxDecoration;
      expect(decoration.color, colors.sidebar, reason: name);
      expect(decoration.border, Border.all(color: colors.border), reason: name);
      expect(decoration.border!.top.width, 1, reason: name);
      expect(
        decoration.border!.top.strokeAlign,
        BorderSide.strokeAlignInside,
        reason: name,
      );
      expect(decoration.borderRadius, BorderRadius.circular(16), reason: name);
      expect(decoration.boxShadow, _floatShadows[name], reason: name);

      final clip = tester.widget<ClipRRect>(
        find.descendant(
          of: find.byType(OcFloat),
          matching: find.byType(ClipRRect),
        ),
      );
      expect(clip.borderRadius, BorderRadius.circular(15), reason: name);
    }
  });

  testWidgets('a_float_takes_the_colour_it_is_given', (tester) async {
    await pumpThemed(
      tester,
      Builder(
        builder: (context) => Center(
          child: SizedBox(
            width: 300,
            height: 200,
            child: OcFloat(
              color: context.oc.chat,
              radius: 12,
              child: const SizedBox.expand(),
            ),
          ),
        ),
      ),
    );

    final decoration =
        tester
                .widget<DecoratedBox>(
                  find.descendant(
                    of: find.byType(OcFloat),
                    matching: find.byType(DecoratedBox),
                  ),
                )
                .decoration
            as BoxDecoration;
    expect(decoration.color, OcColors.dark.chat);
    expect(decoration.borderRadius, BorderRadius.circular(12));
    final clip = tester.widget<ClipRRect>(
      find.descendant(
        of: find.byType(OcFloat),
        matching: find.byType(ClipRRect),
      ),
    );
    expect(clip.borderRadius, BorderRadius.circular(11));
  });

  test('shadows_have_the_spec_values', () {
    for (final (name, colors) in themes) {
      expect(OcShadows.float(colors), _floatShadows[name], reason: name);
      expect(OcShadows.menu(colors), _menuShadows[name], reason: name);
    }
  });

  testWidgets('content_sits_inside_the_border', (tester) async {
    const content = Key('content');
    final notch = ValueNotifier<double>(52);
    addTearDown(notch.dispose);
    final floats = {
      'OcFloat': const OcFloat(child: SizedBox.expand(key: content)),
      'NotchedFloat': NotchedFloat(
        notch: notch,
        child: const SizedBox.expand(key: content),
      ),
    };

    for (final MapEntry(key: name, value: float) in floats.entries) {
      await pumpThemed(tester, surface: _window, _placed(float));

      expect(
        tester.getRect(find.byKey(content)),
        rectMoreOrLessEquals((_origin & _box).deflate(1)),
        reason: name,
      );
    }
  });

  test('notch_path_cuts_the_corner', () {
    final path = _notched(52);

    // The notch itself.
    expect(path.contains(const Offset(590, 10)), isFalse);
    expect(path.contains(const Offset(560, 40)), isFalse);
    // Left of it, under it, and the convex joints beside it.
    expect(path.contains(const Offset(540, 10)), isTrue);
    expect(path.contains(const Offset(590, 70)), isTrue);
    expect(path.contains(const Offset(546, 3)), isFalse);
    expect(path.contains(const Offset(597, 54)), isFalse);
    // The concave corner: centre (564, 36), radius 16.
    expect(path.contains(const Offset(552, 49)), isTrue);
    expect(path.contains(const Offset(547, 40)), isTrue);
    expect(path.contains(const Offset(553, 45)), isFalse);
    expect(path.contains(const Offset(549, 40)), isFalse);
    // The other three corners stay the plain float's.
    expect(path.contains(const Offset(2, 2)), isFalse);
    expect(path.contains(const Offset(2, 398)), isFalse);
    expect(path.contains(const Offset(598, 398)), isFalse);
    expect(path.contains(const Offset(8, 8)), isTrue);
  });

  test('an_inset_moves_every_edge_into_the_page', () {
    final path = _notched(52, inset: 1);

    // The outer edges move in.
    expect(path.contains(const Offset(0.5, 200)), isFalse);
    expect(path.contains(const Offset(1.5, 200)), isTrue);
    expect(path.contains(const Offset(300, 399.5)), isFalse);
    expect(path.contains(const Offset(300, 398.5)), isTrue);
    expect(path.contains(const Offset(599.5, 200)), isFalse);
    expect(path.contains(const Offset(598.5, 200)), isTrue);
    // The notch's edges move out, into the page.
    expect(path.contains(const Offset(547.5, 20)), isFalse);
    expect(path.contains(const Offset(546.5, 20)), isTrue);
    expect(path.contains(const Offset(575, 52.5)), isFalse);
    expect(path.contains(const Offset(575, 53.5)), isTrue);
    // The concave corner keeps its centre, with radius 17.
    expect(path.contains(const Offset(552, 50)), isTrue);
    expect(path.contains(const Offset(553, 46)), isFalse);
  });

  test('notch_path_at_zero_is_the_plain_float', () {
    final path = _notched(0);
    final plain = RRect.fromRectAndRadius(
      Offset.zero & _box,
      const Radius.circular(16),
    );

    // 100 points in each corner's 24 px square, where the shapes could
    // differ; the rest of the box is plainly inside both.
    var checked = 0;
    for (final corner in [
      Offset.zero,
      Offset(_box.width - 24, 0),
      Offset(0, _box.height - 24),
      Offset(_box.width - 24, _box.height - 24),
    ]) {
      for (var i = 0; i < 10; i++) {
        for (var j = 0; j < 10; j++) {
          final point = corner + Offset(i * 2.4 + 1.1, j * 2.4 + 1.3);
          expect(path.contains(point), plain.contains(point), reason: '$point');
          checked++;
        }
      }
    }
    expect(checked, 400);
  });

  test('notch_path_never_breaks_on_the_way', () {
    final centre = _box.center(Offset.zero);
    for (final inset in [0.0, 0.5, 1.0]) {
      final box = (Offset.zero & _box).deflate(inset);
      for (var notch = 0.0; notch <= 52; notch += 0.5) {
        final path = _notched(notch, inset: inset);
        final at = 'notch $notch, inset $inset';

        expect(
          path.getBounds(),
          rectMoreOrLessEquals(box, epsilon: 0.01),
          reason: at,
        );
        expect(path.contains(centre), isTrue, reason: at);
        // Never larger than the plain float.
        expect(path.contains(box.topRight + const Offset(-1, 1)), isFalse);
        // The notch's middle is open once it's wider than the corner.
        if (notch >= 12) {
          expect(
            path.contains(Offset(box.right - notch / 2, box.top + notch / 2)),
            isFalse,
            reason: at,
          );
        }
      }
    }
  });

  testWidgets('clip_uses_the_notch', (tester) async {
    final notch = ValueNotifier<double>(52);
    addTearDown(notch.dispose);
    final boundary = GlobalKey();
    var inside = 0;
    var behind = 0;

    await pumpThemed(
      tester,
      surface: _window,
      background: (c) => c.rail,
      RepaintBoundary(
        key: boundary,
        child: Stack(
          children: [
            Positioned.fill(
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: () => behind++,
                child: ColoredBox(color: OcColors.dark.rail),
              ),
            ),
            _placed(
              NotchedFloat(
                notch: notch,
                child: GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onTap: () => inside++,
                  child: ColoredBox(color: OcColors.dark.text),
                ),
              ),
            ),
          ],
        ),
      ),
    );

    // In the notch, the page neither takes the tap nor paints.
    final inNotch = _origin + const Offset(574, 26);
    await tester.tapAt(inNotch);
    expect(inside, 0);
    expect(behind, 1);
    final inPage = _origin + const Offset(300, 200);
    await tester.tapAt(inPage);
    expect(inside, 1);
    expect(behind, 1);

    var pixels = await _pixels(tester, boundary);
    expect(_near(_pixel(pixels, inNotch), OcColors.dark.rail), isTrue);
    expect(_pixel(pixels, inPage), OcColors.dark.text);

    // Shut, the corner belongs to the page again.
    notch.value = 0;
    await tester.pump();
    await tester.tapAt(inNotch);
    expect(inside, 2);
    expect(behind, 1);
    pixels = await _pixels(tester, boundary);
    expect(_pixel(pixels, inNotch), OcColors.dark.text);

    // And open again, the same as the first time.
    notch.value = 52;
    await tester.pump();
    await tester.tapAt(inNotch);
    expect(inside, 2);
    expect(behind, 2);
    pixels = await _pixels(tester, boundary);
    expect(_near(_pixel(pixels, inNotch), OcColors.dark.rail), isTrue);
  });

  testWidgets('the_notch_follows_its_listenable_without_a_rebuild', (
    tester,
  ) async {
    final notch = ValueNotifier<double>(0);
    addTearDown(notch.dispose);
    final boundary = GlobalKey();
    var builds = 0;

    await pumpThemed(
      tester,
      surface: _window,
      background: (c) => c.rail,
      RepaintBoundary(
        key: boundary,
        child: ColoredBox(
          color: OcColors.dark.rail,
          child: _placed(
            NotchedFloat(
              notch: notch,
              child: Builder(
                builder: (context) {
                  builds++;
                  return ColoredBox(color: OcColors.dark.text);
                },
              ),
            ),
          ),
        ),
      ),
    );
    expect(builds, 1);

    final at = _origin + const Offset(590, 10);
    for (final value in [26.0, 52.0, 10.0, 0.0]) {
      notch.value = value;
      await tester.pump();
      final pixels = await _pixels(tester, boundary);
      // (590, 10) is in the notch from about 12 up.
      expect(
        _near(_pixel(pixels, at), OcColors.dark.rail),
        value >= 12,
        reason: 'notch $value',
      );
    }
    expect(builds, 1);
  });
}
