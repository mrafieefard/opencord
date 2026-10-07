import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/chat_scroll.dart';

Future<ChatScrollController> _pump(
  WidgetTester tester, {
  Duration wheel = const Duration(milliseconds: 140),
}) async {
  final controller = ChatScrollController(wheel: wheel);
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

Future<void> _wheel(WidgetTester tester, double dy) async {
  final pointer = TestPointer(1, PointerDeviceKind.mouse);
  final center = tester.getCenter(find.byType(CustomScrollView));
  await tester.sendEventToBinding(pointer.hover(center));
  await tester.sendEventToBinding(pointer.scroll(Offset(0, dy)));
}

void main() {
  testWidgets('a wheel notch glides instead of jumping (§15)', (tester) async {
    final controller = await _pump(tester);
    final start = controller.position.pixels;

    await _wheel(tester, -100);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    final midway = controller.position.pixels;

    expect(midway, lessThan(start));
    expect(midway, greaterThan(start - 100));
    await tester.pump(const Duration(milliseconds: 300));
    expect(controller.position.pixels, start - 100);
  });

  testWidgets('quick notches add up', (tester) async {
    final controller = await _pump(tester);
    final start = controller.position.pixels;

    await _wheel(tester, -100);
    await tester.pump(const Duration(milliseconds: 30));
    await _wheel(tester, -100);
    // A first frame starts the glide's clock.
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));

    expect(controller.position.pixels, start - 200);
  });

  testWidgets('small touchpad steps follow at once', (tester) async {
    final controller = await _pump(tester);
    final start = controller.position.pixels;

    await _wheel(tester, -6);
    await tester.pump();

    expect(controller.position.pixels, start - 6);
  });

  testWidgets('with reduced motion a notch jumps', (tester) async {
    final controller = await _pump(tester, wheel: Duration.zero);
    final start = controller.position.pixels;

    await _wheel(tester, -100);
    await tester.pump();

    expect(controller.position.pixels, start - 100);
  });
}
