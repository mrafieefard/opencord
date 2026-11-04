import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/motion/oc_spring.dart';

Duration _ms(int milliseconds) => Duration(milliseconds: milliseconds);

void main() {
  const springs = [
    OcSpring.pane,
    OcSpring.paneOut,
    OcSpring.pop,
    OcSpring.tile,
    OcSpring.move,
  ];

  test('each_token_settles_where_the_prototype_does', () {
    const settled = [342, 244, 177, 295, 366];
    for (var i = 0; i < springs.length; i++) {
      expect(springs[i].settle.inMilliseconds, closeTo(settled[i], 4));
    }
  });

  test('settle_is_the_last_moment_more_than_0_4_percent_away', () {
    for (final spring in springs) {
      final settle = spring.settle;
      expect((1 - spring.valueAt(settle)).abs(), greaterThan(0.004));
      for (var ms = settle.inMilliseconds + 1; ms < 1000; ms++) {
        expect((1 - spring.valueAt(_ms(ms))).abs(), lessThanOrEqualTo(0.004));
      }
    }
  });

  test('pane_follows_the_sampled_curve', () {
    const sideSlot = {
      40: 59.9,
      80: 141.0,
      120: 197.6,
      160: 230.4,
      200: 247.6,
      280: 260.3,
    };
    sideSlot.forEach((ms, width) {
      expect(
        264 * OcSpring.pane.valueAt(_ms(ms)),
        closeTo(width, 0.5),
        reason: 'at $ms ms',
      );
    });
  });

  test('pane_out_follows_the_sampled_curve', () {
    const sideSlot = {40: 169.6, 80: 75.3, 120: 29.2, 160: 10.5, 200: 3.5};
    sideSlot.forEach((ms, width) {
      expect(
        264 * (1 - OcSpring.paneOut.valueAt(_ms(ms))),
        closeTo(width, 0.5),
        reason: 'at $ms ms',
      );
    });
  });

  test('pop_follows_the_sampled_menu', () {
    const opacity = {40: .38, 80: .76, 120: .93, 160: .99};
    opacity.forEach((ms, value) {
      expect(
        OcSpring.pop.valueAt(_ms(ms)),
        closeTo(value, 0.01),
        reason: 'at $ms ms',
      );
    });
  });

  test('pop_has_its_small_overshoot_and_pane_has_none', () {
    double highest(OcSpring spring) {
      var highest = 0.0;
      for (var ms = 0; ms <= 500; ms++) {
        final value = spring.valueAt(_ms(ms));
        if (value > highest) highest = value;
      }
      return highest;
    }

    expect(highest(OcSpring.pop), greaterThan(1));
    expect(highest(OcSpring.pop), lessThan(1.004));
    expect(highest(OcSpring.pane), lessThanOrEqualTo(1));
  });

  test('curve_ends_exactly_at_one', () {
    for (final spring in springs) {
      final curve = spring.curve;
      expect(curve.transform(0), 0);
      expect(curve.transform(1), 1);
      expect(
        curve.transform(0.5),
        closeTo(spring.valueAt(spring.settle * 0.5), 1e-9),
      );
      expect(identical(spring.curve, curve), isTrue);
    }
  });

  test('simulation_is_the_same_spring_and_keeps_the_speed_it_is_given', () {
    for (final spring in springs) {
      final simulation = spring.simulation(0, 1, 0);
      for (var ms = 0; ms < spring.settle.inMilliseconds; ms += 8) {
        expect(
          simulation.x(ms / 1000),
          closeTo(spring.valueAt(_ms(ms)), 1e-6),
          reason: 'at $ms ms',
        );
      }
      expect(spring.simulation(0.5, 0, 3).dx(0), closeTo(3, 1e-9));
    }
  });

  test('simulation_ends_at_the_settled_moment_on_the_exact_target', () {
    for (final spring in springs) {
      final simulation = spring.simulation(10, 74, 0);
      final seconds = spring.settle.inMilliseconds / 1000;
      expect(simulation.isDone(seconds), isFalse);
      expect(simulation.isDone(seconds + 0.002), isTrue);
      expect(simulation.x(seconds + 0.002), 74);
      expect(simulation.dx(seconds + 0.002), 0);
    }
  });
}
