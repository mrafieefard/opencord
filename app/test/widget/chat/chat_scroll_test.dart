import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/chat_scroll.dart';

const _rowHeight = 50.0;
const _viewport = Size(300, 500);

/// A chat-shaped scroll view: [older] rows above the anchor (index 0 is the
/// one touching it), [newer] rows below.
class _Chat extends StatelessWidget {
  const _Chat({
    required this.controller,
    required this.older,
    required this.newer,
  });

  static const _center = Key('center');

  final ChatScrollController controller;
  final int older;
  final int newer;

  @override
  Widget build(BuildContext context) => Directionality(
    textDirection: TextDirection.ltr,
    child: CustomScrollView(
      controller: controller,
      center: _center,
      anchor: 1,
      slivers: [
        SliverList.builder(
          itemCount: older,
          itemBuilder: (context, index) =>
              SizedBox(height: _rowHeight, child: Text('old $index')),
        ),
        SliverList.builder(
          key: _center,
          itemCount: newer,
          itemBuilder: (context, index) =>
              SizedBox(height: _rowHeight, child: Text('new $index')),
        ),
      ],
    ),
  );
}

Future<void> _pump(
  WidgetTester tester,
  ChatScrollController controller, {
  required int older,
  int newer = 0,
}) async {
  tester.view.physicalSize = _viewport;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    _Chat(controller: controller, older: older, newer: newer),
  );
}

double _top(WidgetTester tester, String text) =>
    tester.getTopLeft(find.text(text)).dy;

void main() {
  testWidgets('opens at the newest message', (tester) async {
    final controller = ChatScrollController();
    await _pump(tester, controller, older: 40);

    expect(controller.atEnd, isTrue);
    expect(_top(tester, 'old 0'), _viewport.height - _rowHeight);
  });

  testWidgets('follows new messages while at the newest one', (tester) async {
    final controller = ChatScrollController();
    await _pump(tester, controller, older: 40);

    await _pump(tester, controller, older: 40, newer: 3);

    expect(controller.atEnd, isTrue);
    expect(_top(tester, 'new 2'), _viewport.height - _rowHeight);
  });

  testWidgets('stays put while reading older messages', (tester) async {
    final controller = ChatScrollController();
    await _pump(tester, controller, older: 40);
    controller.jumpTo(controller.offset - 300);
    await tester.pump();
    final before = _top(tester, 'old 8');

    // New messages below and a page of history above.
    await _pump(tester, controller, older: 90, newer: 5);

    expect(_top(tester, 'old 8'), before);
    expect(controller.atEnd, isFalse);
  });

  testWidgets('opens with the anchor where it was asked for', (tester) async {
    final controller = ChatScrollController(start: const StartAtAnchor(100));
    await _pump(tester, controller, older: 40, newer: 30);

    expect(_top(tester, 'new 0'), 100);
    expect(controller.atEnd, isFalse);
  });

  testWidgets('an anchor near the end settles at the newest message', (
    tester,
  ) async {
    final controller = ChatScrollController(start: const StartAtAnchor(100));
    await _pump(tester, controller, older: 40, newer: 2);

    expect(controller.atEnd, isTrue);
    expect(_top(tester, 'new 1'), _viewport.height - _rowHeight);
  });

  testWidgets('a short chat sits at the bottom of the view', (tester) async {
    final controller = ChatScrollController();
    await _pump(tester, controller, older: 3);

    expect(_top(tester, 'old 0'), _viewport.height - _rowHeight);
    expect(_top(tester, 'old 2'), _viewport.height - 3 * _rowHeight);
  });

  testWidgets('scrollToEnd brings the newest message back', (tester) async {
    final controller = ChatScrollController();
    await _pump(tester, controller, older: 40, newer: 20);
    controller.jumpTo(0);
    await tester.pump();

    final done = controller.scrollToEnd(
      duration: const Duration(milliseconds: 200),
    );
    await tester.pumpAndSettle();
    await done;

    expect(controller.atEnd, isTrue);
  });
}
