import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/motion/spring_value.dart';
import 'package:opencord/ui/theme/oc_motion.dart';

const _frame = Duration(milliseconds: 16);

Duration _ms(int milliseconds) => Duration(milliseconds: milliseconds);

/// A [SpringValue] that records each value it builds with.
class _Probe {
  final built = <double>[];

  double get value => built.last;

  Widget at(
    double target, {
    OcSpring spring = OcSpring.pane,
    OcSpring? falling,
    Duration delay = Duration.zero,
    Duration fallingDelay = Duration.zero,
    bool reduced = false,
  }) => MediaQuery(
    data: MediaQueryData(disableAnimations: reduced),
    child: SpringValue(
      target: target,
      spring: spring,
      falling: falling,
      delay: delay,
      fallingDelay: fallingDelay,
      builder: (context, value, child) {
        built.add(value);
        return const SizedBox.shrink();
      },
    ),
  );
}

void main() {
  testWidgets('spring_value_is_on_the_curve_frame_by_frame', (tester) async {
    for (final spring in const [
      OcSpring.pane,
      OcSpring.paneOut,
      OcSpring.pop,
      OcSpring.tile,
      OcSpring.move,
    ]) {
      final probe = _Probe();
      await tester.pumpWidget(probe.at(0, spring: spring));
      await tester.pumpWidget(probe.at(1, spring: spring));
      expect(probe.value, 0);

      var t = Duration.zero;
      while (t + _frame < spring.settle) {
        await tester.pump(_frame);
        t += _frame;
        expect(
          probe.value,
          closeTo(spring.valueAt(t), 0.005),
          reason: '$t on ${spring.seconds}',
        );
      }

      await tester.pump(_ms(40));
      expect(probe.value, 1);
      await tester.pumpWidget(const SizedBox.shrink());
    }
  });

  testWidgets('a_falling_target_runs_on_its_own_spring', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(1, falling: OcSpring.paneOut));
    await tester.pumpWidget(probe.at(0, falling: OcSpring.paneOut));

    var t = Duration.zero;
    while (t + _frame < OcSpring.paneOut.settle) {
      await tester.pump(_frame);
      t += _frame;
      expect(
        probe.value,
        closeTo(1 - OcSpring.paneOut.valueAt(t), 0.005),
        reason: 'at $t',
      );
    }
    await tester.pump(_ms(40));
    expect(probe.value, 0);
  });

  testWidgets('falling_defaults_to_the_spring', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(1));
    await tester.pumpWidget(probe.at(0));
    await tester.pump(_ms(80));
    expect(probe.value, closeTo(1 - OcSpring.pane.valueAt(_ms(80)), 0.005));
  });

  testWidgets('it_starts_at_its_first_target_without_moving', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(52));
    expect(probe.value, 52);
    expect(tester.hasRunningAnimations, isFalse);
  });

  testWidgets('retargeting_keeps_the_speed', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(0, falling: OcSpring.paneOut));
    await tester.pumpWidget(probe.at(1, falling: OcSpring.paneOut));
    for (var i = 0; i < 4; i++) {
      await tester.pump(_frame);
    }
    final before = probe.value;
    await tester.pump(_frame);
    final atFlip = probe.value;
    final stepBefore = atFlip - before;
    expect(stepBefore, greaterThan(0));

    await tester.pumpWidget(probe.at(0, falling: OcSpring.paneOut));
    expect(probe.value, atFlip);

    await tester.pump(_frame);
    final stepAfter = probe.value - atFlip;
    expect(stepAfter, greaterThan(0));
    expect(stepAfter, lessThan(stepBefore));

    // Turns round from there, without a step bigger than a full-speed one.
    var last = probe.value;
    var highestStep = 0.0;
    while (tester.hasRunningAnimations) {
      await tester.pump(_frame);
      final step = (probe.value - last).abs();
      if (step > highestStep) highestStep = step;
      last = probe.value;
    }
    expect(highestStep, lessThan(0.2));
    expect(probe.value, 0);
  });

  testWidgets('flipping_over_and_over_ends_exact', (tester) async {
    final probe = _Probe();
    var target = 0.0;
    await tester.pumpWidget(probe.at(target, falling: OcSpring.paneOut));
    for (var i = 0; i < 25; i++) {
      target = 264 - target;
      await tester.pumpWidget(probe.at(target, falling: OcSpring.paneOut));
      await tester.pump(_frame);
      expect(probe.value, inInclusiveRange(0, 264));
    }

    await tester.pump(_ms(400));
    expect(probe.value, target);
    expect(tester.hasRunningAnimations, isFalse);
  });

  testWidgets('delay_holds_the_value', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(0, delay: _ms(50)));
    await tester.pumpWidget(probe.at(1, delay: _ms(50)));

    await tester.pump(_ms(40));
    expect(probe.value, 0);
    await tester.pump(_ms(40));
    expect(probe.value, closeTo(OcSpring.pane.valueAt(_ms(30)), 0.005));
    await tester.pump(_ms(400));
    expect(probe.value, 1);

    // That was the rising side: this fall has no delay of its own.
    await tester.pumpWidget(probe.at(0, delay: _ms(50)));
    await tester.pump(_ms(40));
    expect(probe.value, closeTo(1 - OcSpring.pane.valueAt(_ms(40)), 0.005));
  });

  testWidgets('the_delay_runs_on_the_falling_side_with_the_notchs_numbers', (
    tester,
  ) async {
    final probe = _Probe();
    Widget notch(double target) =>
        probe.at(target, falling: OcSpring.paneOut, fallingDelay: _ms(50));

    // The pane opens: §4.3's notch row.
    await tester.pumpWidget(notch(52));
    await tester.pumpWidget(notch(0));
    await tester.pump(_ms(40));
    expect(probe.value, 52);
    const closing = [39.3, 18.5, 7.3, 2.7];
    for (final size in closing) {
      await tester.pump(_ms(40));
      expect(probe.value, closeTo(size, 0.5));
    }
    await tester.pump(_ms(80));
    expect(probe.value, closeTo(0.3, 0.5));
    await tester.pump(_ms(80));
    expect(probe.value, 0);

    // The pane closes: rising has no delay, and §4.3's other row.
    await tester.pumpWidget(notch(52));
    const opening = [11.8, 27.8, 38.9, 45.4, 48.8];
    for (final size in opening) {
      await tester.pump(_ms(40));
      expect(probe.value, closeTo(size, 0.5));
    }
    await tester.pump(_ms(80));
    expect(probe.value, closeTo(51.3, 0.5));
    await tester.pump(_ms(80));
    expect(probe.value, 52);
  });

  testWidgets('a_delay_leaves_a_moving_spring_moving', (tester) async {
    final probe = _Probe();
    Widget at(double target) =>
        probe.at(target, falling: OcSpring.paneOut, fallingDelay: _ms(48));
    await tester.pumpWidget(at(0));
    await tester.pumpWidget(at(1));
    for (var i = 0; i < 5; i++) {
      await tester.pump(_frame);
    }

    // Held by the delay, the rise that was under way goes on as it was.
    await tester.pumpWidget(at(0));
    await tester.pump(_frame);
    expect(probe.value, closeTo(OcSpring.pane.valueAt(_ms(96)), 1e-6));
    await tester.pump(_frame);
    expect(probe.value, closeTo(OcSpring.pane.valueAt(_ms(112)), 1e-6));

    // Then it turns, from that value and that speed.
    final atTurn = OcSpring.pane.valueAt(_ms(128));
    final speed = OcSpring.pane.simulation(0, 1, 0).dx(0.128);
    await tester.pump(_frame);
    expect(probe.value, closeTo(atTurn, 1e-6));
    await tester.pump(_frame);
    expect(
      probe.value,
      closeTo(OcSpring.paneOut.simulation(atTurn, 0, speed).x(0.016), 1e-6),
    );

    await tester.pump(_ms(400));
    expect(probe.value, 0);
    expect(tester.hasRunningAnimations, isFalse);
  });

  testWidgets('changing_back_during_a_delay_never_moves', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(0, delay: _ms(50)));
    await tester.pumpWidget(probe.at(1, delay: _ms(50)));
    await tester.pump(_ms(16));
    await tester.pumpWidget(probe.at(0, delay: _ms(50)));
    await tester.pump(_ms(400));

    expect(probe.built.every((value) => value == 0), isTrue);
    expect(tester.hasRunningAnimations, isFalse);
  });

  testWidgets('reduced_motion_goes_straight_to_the_target', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(0, reduced: true));
    await tester.pumpWidget(probe.at(1, delay: _ms(50), reduced: true));
    expect(probe.value, 1);
    expect(tester.hasRunningAnimations, isFalse);

    await tester.pumpWidget(probe.at(0, fallingDelay: _ms(50), reduced: true));
    expect(probe.value, 0);
    expect(tester.hasRunningAnimations, isFalse);
  });

  testWidgets('reduced_motion_coming_on_ends_a_moving_spring', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(0));
    await tester.pumpWidget(probe.at(1));
    await tester.pump(_ms(80));
    expect(probe.value, lessThan(1));
    expect(tester.hasRunningAnimations, isTrue);

    await tester.pumpWidget(probe.at(1, reduced: true));
    expect(probe.value, 1);
    expect(tester.hasRunningAnimations, isFalse);
  });

  testWidgets('a_finished_spring_leaves_no_ticker', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(0));
    await tester.pumpWidget(probe.at(1));
    expect(tester.hasRunningAnimations, isTrue);

    await tester.pump(OcSpring.pane.settle);
    await tester.pump(_frame);
    expect(tester.hasRunningAnimations, isFalse);
    expect(probe.value, 1);

    final builds = probe.built.length;
    await tester.pump(_ms(100));
    expect(probe.built.length, builds);
  });

  testWidgets('an_unchanged_target_starts_nothing', (tester) async {
    final probe = _Probe();
    await tester.pumpWidget(probe.at(1));
    await tester.pumpWidget(probe.at(1));
    expect(tester.hasRunningAnimations, isFalse);

    await tester.pumpWidget(probe.at(0));
    await tester.pump(_ms(80));
    // A rebuild in the middle of a spring leaves it on its curve.
    await tester.pumpWidget(probe.at(0));
    await tester.pump(_ms(40));
    expect(probe.value, closeTo(1 - OcSpring.pane.valueAt(_ms(120)), 1e-6));
  });

  testWidgets('it_passes_its_child_to_the_builder', (tester) async {
    const child = SizedBox(key: Key('child'));
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(),
        child: SpringValue(
          target: 1,
          spring: OcSpring.pane,
          builder: (context, value, child) => child!,
          child: child,
        ),
      ),
    );
    expect(find.byKey(const Key('child')), findsOneWidget);
  });

  testWidgets('oc_motion_has_the_plain_durations', (tester) async {
    late OcMotion motion;
    Widget host({required bool reduced}) => MediaQuery(
      data: MediaQueryData(disableAnimations: reduced),
      child: Builder(
        builder: (context) {
          motion = OcMotion.of(context);
          return const SizedBox.shrink();
        },
      ),
    );

    await tester.pumpWidget(host(reduced: false));
    expect(motion.press, _ms(80));
    expect(motion.exit, _ms(130));
    expect(motion.fade, _ms(120));
    expect(motion.turn, _ms(140));
    expect(OcMotion.curveIn, Curves.easeIn);

    await tester.pumpWidget(host(reduced: true));
    expect(motion.press, Duration.zero);
    expect(motion.exit, Duration.zero);
    expect(motion.fade, Duration.zero);
    expect(motion.turn, Duration.zero);
  });
}
