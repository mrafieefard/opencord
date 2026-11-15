import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

void main() {
  // Fails if a colour name is added to or taken from one side only: the
  // core's checks (crates/opencord-plugins/src/blocks.rs, COLORS) and the
  // renderer must know the same names.
  test('the_colour_names_match_the_cores', () {
    final source = File(
      '../../../crates/opencord-plugins/src/blocks.rs',
    ).readAsStringSync();
    final list = RegExp(
      r'pub const COLORS: &\[&str\] = &\[(.*?)\];',
      dotAll: true,
    ).firstMatch(source);
    expect(list, isNotNull);
    final names = [
      for (final match in RegExp(r'"([a-z_]+)"').allMatches(list!.group(1)!))
        match.group(1),
    ];
    expect(blockColorNames, names);
    for (final name in names) {
      expect(darkTheme.colors, contains(name));
    }
  });

  // Fails if names, `#rrggbb` or `#rrggbbaa` (alpha last) read wrong, or if
  // something else is taken for a colour.
  test('colours_by_name_and_hex', () {
    expect(darkTheme.resolveColor('accent'), const Color(0xFFEDEDEF));
    expect(darkTheme.resolveColor('#102030'), const Color(0xFF102030));
    expect(darkTheme.resolveColor('#10203040'), const Color(0x40102030));
    expect(darkTheme.resolveColor('#1020'), isNull);
    expect(darkTheme.resolveColor('chartreuse'), isNull);
    expect(darkTheme.resolveColor(12), isNull);
    expect(
      darkTheme.resolveColor({'from': 'accent', 'to': 'surface'}),
      isNull,
      reason: 'a gradient is not one colour',
    );
  });

  // Fails if a gradient's angle doesn't follow CSS (0 towards the top, 90
  // towards the right, 180 by default), or its ends aren't resolved.
  test('gradients_follow_the_css_angle', () {
    final down =
        darkTheme.resolveGradient({'from': 'accent', 'to': '#000000'})!
            as LinearGradient;
    expect(down.colors, const [Color(0xFFEDEDEF), Color(0xFF000000)]);
    expect(down.begin, Alignment.topCenter);
    expect(down.end, Alignment.bottomCenter);

    final right =
        darkTheme.resolveGradient({
              'from': 'accent',
              'to': 'surface',
              'angle': 90,
            })!
            as LinearGradient;
    _near(right.begin as Alignment, Alignment.centerLeft);
    _near(right.end as Alignment, Alignment.centerRight);

    final diagonal =
        darkTheme.resolveGradient({
              'from': 'accent',
              'to': 'surface',
              'angle': 45,
            })!
            as LinearGradient;
    _near(diagonal.begin as Alignment, Alignment.bottomLeft);
    _near(diagonal.end as Alignment, Alignment.topRight);

    expect(darkTheme.resolveGradient('accent'), isNull);
    expect(darkTheme.resolveGradient({'from': 'nope', 'to': 'accent'}), isNull);
  });
}

void _near(Alignment actual, Alignment expected) {
  expect(
    math.max((actual.x - expected.x).abs(), (actual.y - expected.y).abs()),
    lessThan(1e-9),
    reason: '$actual vs $expected',
  );
}
