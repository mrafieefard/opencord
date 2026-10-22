import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

import '../../support/pump.dart';

void main() {
  Widget box(HoverState state) => SizedBox(
    width: 120,
    height: 40,
    child: Text(
      [
        if (state.hovered) 'hovered',
        if (state.pressed) 'pressed',
        if (state.focused) 'focused',
      ].join(' '),
    ),
  );

  testWidgets('reports taps, double taps and secondary taps', (tester) async {
    var taps = 0;
    var doubleTaps = 0;
    Offset? menuAt;
    await pumpThemed(
      tester,
      Center(
        child: Hoverable(
          onTap: () => taps++,
          onDoubleTap: () => doubleTaps++,
          onSecondaryTap: (position) => menuAt = position,
          builder: (context, state) => box(state),
        ),
      ),
    );
    final center = tester.getCenter(find.byType(Hoverable));

    await tester.tapAt(center);
    await tester.pump(kDoubleTapTimeout + const Duration(milliseconds: 10));
    await tester.tapAt(center);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(center);
    await tester.pump(kDoubleTapTimeout);
    await tester.tapAt(center, buttons: kSecondaryButton);
    await tester.pump();

    expect(taps, 1);
    expect(doubleTaps, 1);
    expect(menuAt, center);
  });

  testWidgets('tracks the mouse hovering', (tester) async {
    await pumpThemed(
      tester,
      Center(
        child: Hoverable(onTap: () {}, builder: (context, state) => box(state)),
      ),
    );
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);

    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(find.byType(Hoverable)));
    await tester.pump();

    expect(find.text('hovered'), findsOneWidget);
  });

  testWidgets('works from the keyboard', (tester) async {
    var taps = 0;
    Offset? menuAt;
    await pumpThemed(
      tester,
      Center(
        child: Hoverable(
          autofocus: true,
          onTap: () => taps++,
          onSecondaryTap: (position) => menuAt = position,
          builder: (context, state) => box(state),
        ),
      ),
    );
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyEvent(LogicalKeyboardKey.space);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.f10);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();

    expect(taps, 2);
    expect(menuAt, tester.getCenter(find.byType(Hoverable)));
  });

  testWidgets('draws a focus ring for keyboard focus only', (tester) async {
    await pumpThemed(
      tester,
      Center(
        child: Hoverable(
          autofocus: true,
          onTap: () {},
          builder: (context, state) => box(state),
        ),
      ),
    );
    await tester.pump();
    final ring = find.byKey(Hoverable.focusRingKey);

    FocusManager.instance.highlightStrategy =
        FocusHighlightStrategy.alwaysTouch;
    await tester.pump();
    final hiddenForTouch = ring.evaluate().isEmpty;
    FocusManager.instance.highlightStrategy =
        FocusHighlightStrategy.alwaysTraditional;
    await tester.pump();
    final shownForKeyboard = ring.evaluate().isNotEmpty;
    FocusManager.instance.highlightStrategy = FocusHighlightStrategy.automatic;

    expect(hiddenForTouch, isTrue);
    expect(shownForKeyboard, isTrue);
  });

  testWidgets('without callbacks it is not focusable', (tester) async {
    await pumpThemed(
      tester,
      Center(child: Hoverable(builder: (context, state) => box(state))),
    );

    final focus = tester.widget<Focus>(
      find
          .descendant(of: find.byType(Hoverable), matching: find.byType(Focus))
          .first,
    );
    expect(focus.canRequestFocus, isFalse);
  });
}
