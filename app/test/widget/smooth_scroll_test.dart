import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/smooth_scroll.dart';

void main() {
  const frame = Duration(milliseconds: 16);

  Future<ScrollController> pumpList(WidgetTester tester) async {
    final controller = ScrollController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: ListView.builder(
          controller: controller,
          itemExtent: 50,
          itemCount: 200,
          itemBuilder: (_, i) => Text('$i'),
        ),
      ),
    );
    return controller;
  }

  PointerScrollEvent wheel(WidgetTester tester, double dy) =>
      PointerScrollEvent(
        position: tester.getCenter(find.byType(ListView)),
        scrollDelta: Offset(0, dy),
      );

  Future<void> settle(WidgetTester tester) async {
    for (var i = 0; i < 40; i++) {
      await tester.pump(frame);
    }
  }

  testWidgets('a wheel step glides over a few frames and lands on the whole '
      'step', (tester) async {
    final controller = await pumpList(tester);
    final smoother = WheelSmoother(dispatch: tester.binding.handlePointerEvent);

    expect(smoother.take(wheel(tester, 120)), isTrue);
    await tester.pump(frame);
    final first = controller.offset;
    expect(first, greaterThan(0));
    expect(first, lessThan(120));

    await tester.pump(frame);
    expect(controller.offset, greaterThan(first));

    await settle(tester);
    expect(controller.offset, closeTo(120, 0.01));
  });

  testWidgets('a step taken mid-glide adds to what is left', (tester) async {
    final controller = await pumpList(tester);
    final smoother = WheelSmoother(dispatch: tester.binding.handlePointerEvent);

    smoother.take(wheel(tester, 120));
    await tester.pump(frame);
    await tester.pump(frame);
    smoother.take(wheel(tester, 120));
    await settle(tester);

    expect(controller.offset, closeTo(240, 0.01));
  });

  testWidgets('turning the wheel back drops what is left of the other way', (
    tester,
  ) async {
    final controller = await pumpList(tester);
    controller.jumpTo(500);
    final smoother = WheelSmoother(dispatch: tester.binding.handlePointerEvent);

    smoother.take(wheel(tester, 120));
    await tester.pump(frame);
    final turned = controller.offset;
    smoother.take(wheel(tester, -60));
    await settle(tester);

    expect(controller.offset, closeTo(turned - 60, 0.01));
  });

  test('trackpads and other events are left to Flutter', () {
    final smoother = WheelSmoother(dispatch: (_) {});

    expect(
      smoother.take(
        const PointerScrollEvent(
          kind: PointerDeviceKind.trackpad,
          scrollDelta: Offset(0, 10),
        ),
      ),
      isFalse,
    );
    expect(smoother.take(const PointerHoverEvent()), isFalse);
  });
}
