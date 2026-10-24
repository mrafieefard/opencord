import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/smooth_scroll.dart';
import 'package:opencord/features/chat/chat_scroll.dart';

const _frame = Duration(milliseconds: 16);

Future<ChatScrollController> _pump(WidgetTester tester) async {
  final controller = ChatScrollController();
  addTearDown(controller.dispose);
  await tester.pumpWidget(
    MaterialApp(
      home: CustomScrollView(
        controller: controller,
        slivers: [
          SliverList.builder(
            itemCount: 200,
            itemBuilder: (context, index) =>
                SizedBox(height: 40, child: Text('$index')),
          ),
        ],
      ),
    ),
  );
  return controller;
}

PointerScrollEvent _notch(WidgetTester tester, double dy) => PointerScrollEvent(
  position: tester.getCenter(find.byType(CustomScrollView)),
  scrollDelta: Offset(0, dy),
);

Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 40; i++) {
    await tester.pump(_frame);
  }
}

void main() {
  // The app glides every wheel notch at the binding (WheelSmoother); the
  // chat once glided notches of its own too, and the two fought: most of
  // each notch was lost.
  testWidgets('a wheel notch moves the chat by the whole notch', (
    tester,
  ) async {
    final controller = await _pump(tester);
    controller.jumpTo(2000);
    final smoother = WheelSmoother(dispatch: tester.binding.handlePointerEvent);

    smoother.take(_notch(tester, -100));
    await _settle(tester);

    expect(controller.position.pixels, closeTo(1900, 0.01));
  });

  testWidgets('quick notches add up in the chat', (tester) async {
    final controller = await _pump(tester);
    controller.jumpTo(2000);
    final smoother = WheelSmoother(dispatch: tester.binding.handlePointerEvent);

    smoother.take(_notch(tester, -100));
    await tester.pump(_frame);
    smoother.take(_notch(tester, -100));
    await _settle(tester);

    expect(controller.position.pixels, closeTo(1800, 0.01));
  });
}
